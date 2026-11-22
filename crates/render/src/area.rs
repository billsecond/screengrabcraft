//! The shape of a capture's area when it isn't one box: several boxes dragged out on one screen,
//! or an outline drawn freehand. The capture is cut to the box round all of it, laid out as it was
//! on the screen, and what's outside the shape is made see-through.

use screengrabcraft_model::raster::checked_len;
use screengrabcraft_model::{IRect, Raster};

/// The most boxes kept in one capture, and the most points of an outline.
pub const MAX_BOXES: usize = 64;
pub const MAX_POINTS: usize = 20_000;

/// What of a picture is kept, in its pixels.
#[derive(Clone, Debug, PartialEq)]
pub enum AreaShape {
    /// Several boxes (each kept whole, wherever they overlap).
    Boxes(Vec<IRect>),
    /// An outline, closed from its last point back to its first; inside it is kept.
    Freehand(Vec<(f32, f32)>),
}

impl AreaShape {
    /// The same shape `dx`, `dy` pixels along (to put it in a picture cut from a larger one).
    pub fn moved(&self, dx: i32, dy: i32) -> AreaShape {
        match self {
            AreaShape::Boxes(b) => AreaShape::Boxes(b.iter().map(|r| IRect::new(r.x.saturating_add(dx), r.y.saturating_add(dy), r.w, r.h)).collect()),
            AreaShape::Freehand(p) => AreaShape::Freehand(p.iter().map(|(x, y)| (x + dx as f32, y + dy as f32)).collect()),
        }
    }

    /// The same shape made `f` times as large (a smaller copy for a preview).
    pub fn scaled(&self, f: f32) -> AreaShape {
        let f = if f.is_finite() && f > 0.0 { f } else { 1.0 };
        match self {
            AreaShape::Boxes(b) => AreaShape::Boxes(
                b.iter()
                    .map(|r| {
                        let (x0, y0) = ((r.x as f32 * f).floor(), (r.y as f32 * f).floor());
                        let (x1, y1) = ((r.right() as f32 * f).ceil(), (r.bottom() as f32 * f).ceil());
                        IRect::new(x0 as i32, y0 as i32, (x1 - x0).max(1.0) as u32, (y1 - y0).max(1.0) as u32)
                    })
                    .collect(),
            ),
            AreaShape::Freehand(p) => AreaShape::Freehand(p.iter().map(|(x, y)| (x * f, y * f)).collect()),
        }
    }

    /// The smallest box holding all of it, in whole pixels; none when it holds nothing.
    pub fn bounds(&self) -> Option<IRect> {
        match self {
            AreaShape::Boxes(b) => b.iter().filter(|r| !r.is_empty()).copied().reduce(IRect::union),
            AreaShape::Freehand(p) => {
                let mut pts = p.iter().filter(|(x, y)| x.is_finite() && y.is_finite());
                let first = pts.next()?;
                let (mut x0, mut y0, mut x1, mut y1) = (first.0, first.1, first.0, first.1);
                for (x, y) in pts {
                    (x0, y0, x1, y1) = (x0.min(*x), y0.min(*y), x1.max(*x), y1.max(*y));
                }
                let (x0, y0, x1, y1) = (x0.floor(), y0.floor(), x1.ceil(), y1.ceil());
                let (w, h) = (x1 - x0, y1 - y0);
                (w >= 1.0 && h >= 1.0 && w < 1e9 && h < 1e9 && x0.abs() < 1e9 && y0.abs() < 1e9)
                    .then(|| IRect::new(x0 as i32, y0 as i32, w as u32, h as u32))
            }
        }
    }
}

/// How much of each pixel of a `w` × `h` picture `shape` covers, 0 (none) to 255 (all), a byte a
/// pixel; an outline's edge is smoothed. None when the shape covers none of it (or the picture is
/// too large).
pub fn coverage(shape: &AreaShape, w: u32, h: u32) -> Option<Vec<u8>> {
    let len = checked_len(w, h).ok()? / 4;
    if len == 0 {
        return None;
    }
    let mut out = match shape {
        AreaShape::Boxes(boxes) => {
            let mut out = vec![0u8; len];
            let all = IRect::new(0, 0, w, h);
            for r in boxes.iter().take(MAX_BOXES).filter_map(|b| b.intersect(all)) {
                for y in r.y.max(0) as usize..(r.bottom().max(0) as usize).min(h as usize) {
                    let row = y * w as usize;
                    if let Some(s) = out.get_mut(row + r.x.max(0) as usize..row + (r.right().max(0) as usize).min(w as usize)) {
                        s.fill(255);
                    }
                }
            }
            out
        }
        AreaShape::Freehand(points) => {
            let mut pts = points.iter().filter(|(x, y)| x.is_finite() && y.is_finite() && x.abs() < 1e7 && y.abs() < 1e7).take(MAX_POINTS);
            let first = pts.next()?;
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(first.0, first.1);
            for (x, y) in pts {
                pb.line_to(*x, *y);
            }
            pb.close();
            let path = pb.finish()?;
            let mut mask = tiny_skia::Mask::new(w, h)?;
            mask.fill_path(&path, tiny_skia::FillRule::Winding, true, tiny_skia::Transform::identity());
            mask.data().to_vec()
        }
    };
    if out.iter().all(|c| *c == 0) {
        return None;
    }
    out.truncate(len);
    Some(out)
}

/// `pic` with what's outside `shape` (in its pixels) made see-through. A shape that covers none of
/// it leaves it as it was: a capture is never thrown away as nothing.
pub fn keep_inside(pic: &Raster, shape: &AreaShape) -> Raster {
    let Some(cover) = coverage(shape, pic.width(), pic.height()) else { return pic.clone() };
    let mut out = pic.clone();
    for (px, c) in out.pixels_mut().as_chunks_mut::<4>().0.iter_mut().zip(cover) {
        px[3] = ((u16::from(px[3]) * u16::from(c) + 127) / 255) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Color;

    fn alpha(r: &Raster, x: u32, y: u32) -> u8 {
        r.pixel(x, y).map_or(0, |c| c.a)
    }

    #[test]
    fn boxes_keep_what_is_in_any_of_them_and_nothing_else() {
        let pic = Raster::filled(100, 60, Color::rgb(200, 30, 30)).unwrap();
        let shape = AreaShape::Boxes(vec![IRect::new(0, 0, 30, 20), IRect::new(60, 30, 40, 30), IRect::new(20, 10, 20, 20)]);
        let out = keep_inside(&pic, &shape);
        assert_eq!((out.width(), out.height()), (100, 60));
        for (x, y, kept) in [(0, 0, true), (29, 19, true), (30, 0, false), (35, 25, true), (50, 40, false), (99, 59, true), (59, 30, false)] {
            assert_eq!(alpha(&out, x, y) == 255, kept, "({x}, {y})");
            assert!(alpha(&out, x, y) == 255 || alpha(&out, x, y) == 0);
        }
        // The colours stay; only how see-through changes.
        assert_eq!(out.pixel(10, 10).map(|c| (c.r, c.g, c.b)), Some((200, 30, 30)));
        assert_eq!(shape.bounds(), Some(IRect::new(0, 0, 100, 60)));
        assert_eq!(shape.moved(-10, 5).bounds(), Some(IRect::new(-10, 5, 100, 60)));
    }

    #[test]
    fn an_outline_keeps_its_inside_with_a_smooth_edge() {
        let pic = Raster::filled(100, 100, Color::WHITE).unwrap();
        // A triangle: its corners at the top left, the top right and the bottom left.
        let shape = AreaShape::Freehand(vec![(0.0, 0.0), (100.0, 0.0), (0.0, 100.0)]);
        let out = keep_inside(&pic, &shape);
        assert_eq!(alpha(&out, 10, 10), 255);
        assert_eq!(alpha(&out, 90, 90), 0);
        // On the slanted edge, partly.
        let edge: Vec<u8> = (0..100).map(|x| alpha(&out, x, 99 - x)).collect();
        assert!(edge.iter().any(|a| *a > 0 && *a < 255), "{edge:?}");
        assert_eq!(shape.bounds(), Some(IRect::new(0, 0, 100, 100)));
        // A smaller copy for a preview keeps its shape.
        let small = coverage(&shape.scaled(0.5), 50, 50).unwrap();
        assert_eq!((small[5 * 50 + 5], small[45 * 50 + 45]), (255, 0));
    }

    #[test]
    fn a_shape_that_keeps_nothing_leaves_the_picture_whole_and_nonsense_never_panics() {
        let pic = Raster::filled(20, 20, Color::WHITE).unwrap();
        for shape in [
            AreaShape::Boxes(Vec::new()),
            AreaShape::Boxes(vec![IRect::new(500, 500, 10, 10), IRect::new(i32::MAX, i32::MIN, u32::MAX, u32::MAX)]),
            AreaShape::Freehand(Vec::new()),
            AreaShape::Freehand(vec![(f32::NAN, 1.0), (f32::INFINITY, 2.0)]),
            AreaShape::Freehand(vec![(1.0, 1.0), (5.0, 1.0)]),
            AreaShape::Freehand(vec![(-1e30, -1e30), (1e30, 0.0), (0.0, 1e30)]),
        ] {
            let out = keep_inside(&pic, &shape);
            assert!(out.same_pixels(&pic), "{shape:?}");
            let _ = shape.bounds();
            let _ = shape.scaled(f32::NAN).moved(i32::MAX, i32::MIN);
        }
        // A box larger than the picture keeps all of it.
        let all = keep_inside(&pic, &AreaShape::Boxes(vec![IRect::new(-5, -5, u32::MAX, u32::MAX)]));
        assert_eq!(alpha(&all, 19, 19), 255);
        assert!(coverage(&AreaShape::Boxes(vec![IRect::new(0, 0, 1, 1)]), 0, 10).is_none());
        assert!(coverage(&AreaShape::Boxes(vec![IRect::new(0, 0, 1, 1)]), u32::MAX, u32::MAX).is_none());
    }
}
