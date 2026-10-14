//! Points and rectangles in picture pixels (`f32`, y down, the picture's top left at 0,0).
//!
//! Everything that comes from outside (a project file, a command's parameters) can hold NaN or
//! infinities; [`Pt::finite`] and [`Rect::finite`] replace those with zero so that no later sum
//! or comparison sees them.

use serde::{Deserialize, Serialize};

/// A point in picture pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

/// Coordinates are clamped to this, so that sums and squares stay finite.
pub const COORD_LIMIT: f32 = 1.0e7;

fn clean(v: f32) -> f32 {
    if v.is_finite() { v.clamp(-COORD_LIMIT, COORD_LIMIT) } else { 0.0 }
}

impl Pt {
    pub const ZERO: Pt = Pt { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Pt {
        Pt { x, y }
    }

    /// The point with NaN and infinities replaced by zero and huge values clamped.
    pub fn finite(self) -> Pt {
        Pt { x: clean(self.x), y: clean(self.y) }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn add(self, d: Pt) -> Pt {
        Pt { x: self.x + d.x, y: self.y + d.y }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn sub(self, o: Pt) -> Pt {
        Pt { x: self.x - o.x, y: self.y - o.y }
    }

    pub fn scale(self, s: f32) -> Pt {
        Pt { x: self.x * s, y: self.y * s }
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn distance(self, o: Pt) -> f32 {
        self.sub(o).length()
    }

    pub fn lerp(self, o: Pt, t: f32) -> Pt {
        Pt { x: self.x + (o.x - self.x) * t, y: self.y + (o.y - self.y) * t }
    }
}

/// Distance from `p` to the segment `a`–`b`.
pub fn distance_to_segment(p: Pt, a: Pt, b: Pt) -> f32 {
    let ab = b.sub(a);
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 <= f32::EPSILON {
        return p.distance(a);
    }
    let t = ((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2;
    p.distance(a.lerp(b, t.clamp(0.0, 1.0)))
}

/// Distance from `p` to a polyline (a single point is a dot).
pub fn distance_to_polyline(p: Pt, points: &[Pt]) -> f32 {
    match points {
        [] => f32::INFINITY,
        [only] => p.distance(*only),
        _ => points
            .windows(2)
            .map(|w| match w {
                [a, b] => distance_to_segment(p, *a, *b),
                _ => f32::INFINITY,
            })
            .fold(f32::INFINITY, f32::min),
    }
}

/// An axis-aligned rectangle: its top left corner, width and height. Kept normalized (width and
/// height never negative) by everything that makes one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    /// The rectangle with two opposite corners at `a` and `b`, whichever way round.
    pub fn from_points(a: Pt, b: Pt) -> Rect {
        let (a, b) = (a.finite(), b.finite());
        Rect { x: a.x.min(b.x), y: a.y.min(b.y), w: (a.x - b.x).abs(), h: (a.y - b.y).abs() }
    }

    /// Finite, and with a negative width or height turned round.
    pub fn finite(self) -> Rect {
        let (x, y, w, h) = (clean(self.x), clean(self.y), clean(self.w), clean(self.h));
        Rect { x: if w < 0.0 { x + w } else { x }, y: if h < 0.0 { y + h } else { y }, w: w.abs(), h: h.abs() }
    }

    pub fn min(self) -> Pt {
        Pt { x: self.x, y: self.y }
    }

    pub fn max(self) -> Pt {
        Pt { x: self.x + self.w, y: self.y + self.h }
    }

    pub fn center(self) -> Pt {
        Pt { x: self.x + self.w / 2.0, y: self.y + self.h / 2.0 }
    }

    pub fn right(self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(self) -> f32 {
        self.y + self.h
    }

    pub fn is_empty(self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    pub fn contains(self, p: Pt) -> bool {
        p.x >= self.x && p.y >= self.y && p.x <= self.right() && p.y <= self.bottom()
    }

    /// Grown by `d` on every side (shrunk when negative, never past nothing).
    pub fn inflate(self, d: f32) -> Rect {
        let w = (self.w + 2.0 * d).max(0.0);
        let h = (self.h + 2.0 * d).max(0.0);
        let c = self.center();
        Rect { x: c.x - w / 2.0, y: c.y - h / 2.0, w, h }
    }

    pub fn translate(self, d: Pt) -> Rect {
        Rect { x: self.x + d.x, y: self.y + d.y, ..self }
    }

    pub fn union(self, o: Rect) -> Rect {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect { x, y, w: self.right().max(o.right()) - x, h: self.bottom().max(o.bottom()) - y }
    }

    /// The overlap of the two, if they overlap.
    pub fn intersect(self, o: Rect) -> Option<Rect> {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        (r > x && b > y).then_some(Rect { x, y, w: r - x, h: b - y })
    }

    /// The bounds of a set of points (an empty rectangle at the origin for none).
    pub fn bounding(points: &[Pt]) -> Rect {
        let Some(first) = points.first() else { return Rect::default() };
        let mut r = Rect { x: first.x, y: first.y, w: 0.0, h: 0.0 };
        for p in points.iter().skip(1) {
            r = r.union(Rect { x: p.x, y: p.y, w: 0.0, h: 0.0 });
        }
        r
    }

    /// The four corners: top left, top right, bottom right, bottom left.
    pub fn corners(self) -> [Pt; 4] {
        [Pt::new(self.x, self.y), Pt::new(self.right(), self.y), Pt::new(self.right(), self.bottom()), Pt::new(self.x, self.bottom())]
    }

    /// Whole pixels covering this rectangle, clipped to a `width`×`height` picture: `(x, y, w, h)`,
    /// or none when nothing of it is on the picture.
    pub fn pixel_bounds(self, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
        let r = self.finite();
        let x0 = r.x.floor().clamp(0.0, width as f32) as u32;
        let y0 = r.y.floor().clamp(0.0, height as f32) as u32;
        let x1 = r.right().ceil().clamp(0.0, width as f32) as u32;
        let y1 = r.bottom().ceil().clamp(0.0, height as f32) as u32;
        (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
    }
}

/// A rectangle in whole pixels on the desktop (monitors may sit at negative coordinates).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl IRect {
    pub const fn new(x: i32, y: i32, w: u32, h: u32) -> IRect {
        IRect { x, y, w, h }
    }

    pub fn right(self) -> i64 {
        i64::from(self.x) + i64::from(self.w)
    }

    pub fn bottom(self) -> i64 {
        i64::from(self.y) + i64::from(self.h)
    }

    pub fn is_empty(self) -> bool {
        self.w == 0 || self.h == 0
    }

    pub fn contains(self, x: i64, y: i64) -> bool {
        x >= i64::from(self.x) && y >= i64::from(self.y) && x < self.right() && y < self.bottom()
    }

    /// The overlap of the two, if they overlap.
    pub fn intersect(self, o: IRect) -> Option<IRect> {
        let x = i64::from(self.x).max(i64::from(o.x));
        let y = i64::from(self.y).max(i64::from(o.y));
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        if r <= x || b <= y {
            return None;
        }
        Some(IRect { x: i32::try_from(x).ok()?, y: i32::try_from(y).ok()?, w: u32::try_from(r - x).ok()?, h: u32::try_from(b - y).ok()? })
    }

    /// The smallest rectangle holding both.
    pub fn union(self, o: IRect) -> IRect {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return self;
        }
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        let r = self.right().max(o.right());
        let b = self.bottom().max(o.bottom());
        IRect { x, y, w: u32::try_from(r - i64::from(x)).unwrap_or(u32::MAX), h: u32::try_from(b - i64::from(y)).unwrap_or(u32::MAX) }
    }

    /// The rectangle spanned by two desktop pixels, both included.
    pub fn spanning(ax: i64, ay: i64, bx: i64, by: i64) -> IRect {
        let (x0, x1) = (ax.min(bx), ax.max(bx));
        let (y0, y1) = (ay.min(by), ay.max(by));
        let clamp = |v: i64| i32::try_from(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(0);
        IRect { x: clamp(x0), y: clamp(y0), w: u32::try_from(x1 - x0 + 1).unwrap_or(u32::MAX), h: u32::try_from(y1 - y0 + 1).unwrap_or(u32::MAX) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects_are_normalized_and_finite() {
        let r = Rect::from_points(Pt::new(10.0, 20.0), Pt::new(4.0, 2.0));
        assert_eq!(r, Rect::new(4.0, 2.0, 6.0, 18.0));
        assert_eq!(Rect::new(10.0, 10.0, -4.0, -2.0).finite(), Rect::new(6.0, 8.0, 4.0, 2.0));
        assert_eq!(Rect::new(f32::NAN, f32::INFINITY, 1.0, 1.0).finite(), Rect::new(0.0, 0.0, 1.0, 1.0));
        assert!(Rect::new(0.0, 0.0, 0.0, 5.0).is_empty());
    }

    #[test]
    fn segment_distance() {
        let (a, b) = (Pt::new(0.0, 0.0), Pt::new(10.0, 0.0));
        assert_eq!(distance_to_segment(Pt::new(5.0, 3.0), a, b), 3.0);
        assert_eq!(distance_to_segment(Pt::new(-3.0, 4.0), a, b), 5.0);
        assert_eq!(distance_to_segment(Pt::new(1.0, 1.0), a, a), 2f32.sqrt());
        assert_eq!(distance_to_polyline(Pt::new(0.0, 0.0), &[]), f32::INFINITY);
    }

    #[test]
    fn pixel_bounds_clip_to_the_picture() {
        assert_eq!(Rect::new(-5.0, -5.0, 10.2, 10.0).pixel_bounds(100, 100), Some((0, 0, 6, 5)));
        assert_eq!(Rect::new(95.5, 0.0, 10.0, 1.0).pixel_bounds(100, 100), Some((95, 0, 5, 1)));
        assert_eq!(Rect::new(200.0, 0.0, 10.0, 10.0).pixel_bounds(100, 100), None);
    }

    #[test]
    fn integer_rects() {
        let a = IRect::new(-1920, 0, 1920, 1080);
        let b = IRect::new(0, 0, 2560, 1440);
        assert_eq!(a.union(b), IRect::new(-1920, 0, 4480, 1440));
        assert_eq!(a.intersect(b), None);
        assert_eq!(IRect::new(0, 0, 10, 10).intersect(IRect::new(5, 5, 10, 10)), Some(IRect::new(5, 5, 5, 5)));
        assert_eq!(IRect::spanning(10, 10, 5, 2), IRect::new(5, 2, 6, 9));
        assert!(a.contains(-1, 1079));
        assert!(!a.contains(0, 0));
    }
}
