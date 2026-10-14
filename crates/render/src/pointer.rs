//! A mouse pointer drawn into a capture (Settings › Include the mouse pointer): systems don't
//! hand programs the pointer's own picture alongside the screen's, so a plain arrow is drawn
//! where it was, at the monitor's scale.

use screengrabcraft_model::Raster;
use tiny_skia::{FillRule, Paint, PathBuilder, Stroke, Transform};

use crate::canvas;

/// Draws an arrow pointer with its tip at `(x, y)` (the picture's pixels), `scale` times the size
/// of a 1x pointer.
pub fn draw(r: &Raster, x: f32, y: f32, scale: f32) -> Option<Raster> {
    let mut pix = canvas::to_pixmap(r)?;
    let s = if scale.is_finite() { scale.clamp(0.5, 4.0) } else { 1.0 };
    let fix = |v: f32| if v.is_finite() { v.clamp(-1.0e6, 1.0e6) } else { 0.0 };
    let (x, y) = (fix(x), fix(y));
    // The classic arrow, in a 12×19 box, tip at the origin.
    let pts = [(0.0, 0.0), (0.0, 16.0), (4.0, 12.5), (7.0, 19.0), (9.5, 18.0), (6.5, 11.5), (11.5, 11.5)];
    let mut pb = PathBuilder::new();
    for (i, (px, py)) in pts.iter().enumerate() {
        let (qx, qy) = (x + px * s, y + py * s);
        if i == 0 {
            pb.move_to(qx, qy);
        } else {
            pb.line_to(qx, qy);
        }
    }
    pb.close();
    let path = pb.finish()?;
    let mut white = Paint::default();
    white.set_color_rgba8(255, 255, 255, 255);
    white.anti_alias = true;
    let mut black = Paint::default();
    black.set_color_rgba8(0, 0, 0, 255);
    black.anti_alias = true;
    pix.fill_path(&path, &black, FillRule::Winding, Transform::identity(), None);
    pix.stroke_path(
        &path,
        &black,
        &Stroke { width: 1.6 * s, line_join: tiny_skia::LineJoin::Round, ..Stroke::default() },
        Transform::identity(),
        None,
    );
    // White inside the black outline.
    let inner = path.transform(Transform::from_translate(x, y).pre_scale(0.78, 0.78).pre_translate(-x + 0.6 * s, -y + 2.2 * s))?;
    pix.fill_path(&inner, &white, FillRule::Winding, Transform::identity(), None);
    canvas::from_pixmap(&pix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Color;

    #[test]
    fn draws_at_the_tip() {
        let r = Raster::filled(60, 60, Color::rgb(100, 150, 200)).unwrap();
        let out = draw(&r, 10.0, 10.0, 1.0).unwrap();
        assert_ne!(out.pixel(11, 14), r.pixel(11, 14), "the arrow is drawn below and right of the tip");
        assert_eq!(out.pixel(50, 50), r.pixel(50, 50));
        assert!(draw(&r, -100.0, f32::NAN, f32::INFINITY).is_some());
    }
}
