//! Stamps: small pictograms (a check mark, a cross, a star…) drawn in code, filling their box.

use screengrabcraft_model::{Color, Pt, Rect, StampKind, Style};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::draw::{centered_label, skia_color};

fn paint(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(skia_color(c, 1.0));
    p.anti_alias = true;
    p
}

/// The colour a stamp is filled with when it is first placed.
pub fn default_fill(kind: StampKind) -> Color {
    match kind {
        StampKind::Check => Color::rgb(0x2E, 0x9E, 0x4F),
        StampKind::Cross | StampKind::Heart => Color::rgb(0xE5, 0x39, 0x35),
        StampKind::Star => Color::rgb(0xFF, 0xB3, 0x00),
        StampKind::Question | StampKind::Info => Color::rgb(0x1E, 0x88, 0xE5),
        StampKind::Warning => Color::rgb(0xFB, 0x8C, 0x00),
    }
}

/// Draws `kind` filling the square inside `rect`.
pub fn draw(c: &mut Pixmap, t: Transform, rect: Rect, kind: StampKind, style: &Style) {
    let size = rect.w.min(rect.h).max(2.0);
    let center = rect.center();
    let r = size / 2.0;
    let fill = paint(if style.fill.a > 0 { style.fill } else { default_fill(kind) });
    let ink = if style.stroke.a > 0 { style.stroke } else { Color::WHITE };
    let border = |c: &mut Pixmap, path: &tiny_skia::Path| {
        if style.width > 0.0 && style.stroke.a > 0 {
            let s = Stroke { width: style.width.min(size * 0.12), line_join: LineJoin::Round, ..Stroke::default() };
            c.stroke_path(path, &paint(style.stroke), &s, t, None);
        }
    };
    let mark = Stroke { width: size * 0.12, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    let at = |x: f32, y: f32| Pt::new(center.x + x * r, center.y + y * r);
    match kind {
        StampKind::Check | StampKind::Cross | StampKind::Question | StampKind::Info => {
            let Some(disc) = PathBuilder::from_circle(center.x, center.y, r * 0.96) else { return };
            c.fill_path(&disc, &fill, FillRule::Winding, t, None);
            border(c, &disc);
            let mut pb = PathBuilder::new();
            match kind {
                StampKind::Check => {
                    let (a, b, d) = (at(-0.45, 0.02), at(-0.12, 0.36), at(0.5, -0.34));
                    pb.move_to(a.x, a.y);
                    pb.line_to(b.x, b.y);
                    pb.line_to(d.x, d.y);
                }
                StampKind::Cross => {
                    let (a, b, d, e) = (at(-0.38, -0.38), at(0.38, 0.38), at(0.38, -0.38), at(-0.38, 0.38));
                    pb.move_to(a.x, a.y);
                    pb.line_to(b.x, b.y);
                    pb.move_to(d.x, d.y);
                    pb.line_to(e.x, e.y);
                }
                _ => {
                    let label = if kind == StampKind::Question { "?" } else { "i" };
                    let s = Style { font_size: size * 0.62, bold: true, mono: false, italic: false, ..*style };
                    if let Some(glyph) = centered_label(label, &s, center) {
                        c.fill_path(&glyph, &paint(ink), FillRule::Winding, t, None);
                    }
                    return;
                }
            }
            if let Some(path) = pb.finish() {
                c.stroke_path(&path, &paint(ink), &mark, t, None);
            }
        }
        StampKind::Star => {
            let mut pb = PathBuilder::new();
            for i in 0..10 {
                let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                let rr = if i % 2 == 0 { 0.98 } else { 0.42 };
                let p = at(angle.cos() * rr, angle.sin() * rr + 0.06);
                if i == 0 {
                    pb.move_to(p.x, p.y);
                } else {
                    pb.line_to(p.x, p.y);
                }
            }
            pb.close();
            if let Some(star) = pb.finish() {
                c.fill_path(&star, &fill, FillRule::Winding, t, None);
                border(c, &star);
            }
        }
        StampKind::Warning => {
            let mut pb = PathBuilder::new();
            let (top, left, right) = (at(0.0, -0.88), at(-0.96, 0.8), at(0.96, 0.8));
            pb.move_to(top.x, top.y);
            pb.line_to(right.x, right.y);
            pb.line_to(left.x, left.y);
            pb.close();
            if let Some(tri) = pb.finish() {
                let rounder = Stroke { width: size * 0.1, line_join: LineJoin::Round, ..Stroke::default() };
                c.fill_path(&tri, &fill, FillRule::Winding, t, None);
                c.stroke_path(&tri, &fill, &rounder, t, None);
                border(c, &tri);
            }
            let s = Style { font_size: size * 0.55, bold: true, mono: false, italic: false, ..*style };
            if let Some(glyph) = centered_label("!", &s, at(0.0, 0.18)) {
                c.fill_path(&glyph, &paint(ink), FillRule::Winding, t, None);
            }
        }
        StampKind::Heart => {
            let mut pb = PathBuilder::new();
            let (b, top) = (at(0.0, 0.9), at(0.0, -0.45));
            pb.move_to(b.x, b.y);
            let (c1, c2, l) = (at(-0.6, 0.45), at(-1.05, 0.05), at(-0.95, -0.35));
            pb.cubic_to(c1.x, c1.y, c2.x, c2.y, l.x, l.y);
            let (c1, c2) = (at(-0.85, -0.95), at(-0.15, -0.95));
            pb.cubic_to(c1.x, c1.y, c2.x, c2.y, top.x, top.y);
            let (c1, c2, rr) = (at(0.15, -0.95), at(0.85, -0.95), at(0.95, -0.35));
            pb.cubic_to(c1.x, c1.y, c2.x, c2.y, rr.x, rr.y);
            let (c1, c2) = (at(1.05, 0.05), at(0.6, 0.45));
            pb.cubic_to(c1.x, c1.y, c2.x, c2.y, b.x, b.y);
            pb.close();
            if let Some(heart) = pb.finish() {
                c.fill_path(&heart, &fill, FillRule::Winding, t, None);
                border(c, &heart);
            }
        }
    }
}
