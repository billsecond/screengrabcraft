//! A mouse pointer drawn into a capture (Settings › Include the mouse pointer): systems don't
//! hand programs the pointer's own picture alongside the screen's, so a plain arrow is drawn
//! where it was, at the monitor's scale.

use screengrabcraft_model::{Color, Document, Kind, Pt, Raster, Rect, Shape, Style};
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

/// The pointer alone, `scale` times a 1x pointer's size, on a see-through picture just big
/// enough for it and its outline; and where its tip is in that picture.
pub fn sprite(scale: f32) -> Option<(Raster, Pt)> {
    let s = if scale.is_finite() { scale.clamp(0.5, 4.0) } else { 1.0 };
    let pad = (2.0 * s).ceil();
    let (w, h) = ((12.0 * s + 2.0 * pad).ceil() as u32, (19.5 * s + 2.0 * pad).ceil() as u32);
    let blank = Raster::filled(w, h, Color::TRANSPARENT).ok()?;
    Some((draw(&blank, pad, pad, s)?, Pt::new(pad, pad)))
}

/// A capture with the pointer as an annotation of its own rather than drawn in: `pic` as the
/// picture, and the pointer as a picture annotation with its tip at `(x, y)`, to be moved or
/// deleted in the editor. The capture service writes it as the capture's side file.
pub fn as_document(pic: &Raster, x: f32, y: f32, scale: f32) -> Option<Document> {
    let (pointer, tip) = sprite(scale)?;
    if !(x.is_finite() && y.is_finite()) {
        return None;
    }
    let mut doc = Document::new(pic.clone());
    let rect = Rect::new(x - tip.x, y - tip.y, pointer.width() as f32, pointer.height() as f32);
    let key = doc.add_image(pointer).ok()?;
    let style = Style { shadow: false, width: 0.0, ..Kind::Image.default_style() };
    doc.add(Shape::Image { rect, image: key }, style).ok()?;
    Some(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Color;

    /// The pointer as an annotation: its picture drawn where the pointer was, the same as drawn
    /// in, and the capture itself left as it was.
    #[test]
    fn the_pointer_can_be_an_annotation_of_its_own() {
        let r = Raster::filled(60, 60, Color::rgb(100, 150, 200)).unwrap();
        let doc = as_document(&r, 10.0, 12.0, 1.0).unwrap();
        assert!(doc.base.same_pixels(&r), "the capture without it");
        assert_eq!(doc.objects.len(), 1);
        let Some(Shape::Image { rect, .. }) = doc.objects.first().map(|o| o.shape.clone()) else { panic!("a picture annotation") };
        assert!(rect.x < 10.0 && rect.y < 12.0 && rect.right() > 20.0 && rect.bottom() > 30.0, "round the tip: {rect:?}");
        let flat = crate::render(&doc).unwrap();
        let drawn = draw(&r, 10.0, 12.0, 1.0).unwrap();
        for (x, y) in [(11, 16), (13, 25), (40, 40)] {
            assert_eq!(flat.pixel(x, y), drawn.pixel(x, y), "at {x}, {y}");
        }
        assert!(as_document(&r, f32::NAN, 1.0, 1.0).is_none());
        let (big, tip) = sprite(f32::INFINITY).unwrap();
        assert!(big.width() < 40 && tip.x > 0.0);
    }

    #[test]
    fn draws_at_the_tip() {
        let r = Raster::filled(60, 60, Color::rgb(100, 150, 200)).unwrap();
        let out = draw(&r, 10.0, 10.0, 1.0).unwrap();
        assert_ne!(out.pixel(11, 14), r.pixel(11, 14), "the arrow is drawn below and right of the tip");
        assert_eq!(out.pixel(50, 50), r.pixel(50, 50));
        assert!(draw(&r, -100.0, f32::NAN, f32::INFINITY).is_some());
    }
}
