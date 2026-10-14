//! Pen and paper for the icon artwork.
//!
//! A drawing is written in a 1000-unit square, y down, as curves: lists of points along straight
//! lines, cubic Béziers and arcs. They are filled and stroked with tiny-skia onto a canvas that
//! knows only three things a point can be: the field (the app colour), paper or ink. Each of the
//! three is painted in a channel of its own, so a pixel is whichever covers most of it.

use tiny_skia::{Color, FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform};

/// A point of the 1000-unit square.
pub type Pt = (f32, f32);

/// What a point of the artwork is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Field,
    Paper,
    Ink,
}

impl Part {
    /// The colour it is painted in on the canvas: a channel each.
    fn colour(self) -> Color {
        match self {
            Part::Field => Color::from_rgba8(255, 0, 0, 255),
            Part::Paper => Color::from_rgba8(0, 255, 0, 255),
            Part::Ink => Color::from_rgba8(0, 0, 255, 255),
        }
    }
}

/// Points along a cubic Bézier from `p0` to `p3`, leaving `p0` out.
fn bezier(p0: Pt, p1: Pt, p2: Pt, p3: Pt, n: usize) -> impl Iterator<Item = Pt> {
    (1..=n).map(move |i| {
        let t = i as f32 / n as f32;
        let s = 1.0 - t;
        let (a, b, c, d) = (s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t);
        (a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1)
    })
}

fn lerp(p: Pt, q: Pt, t: f32) -> Pt {
    (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t)
}

fn dist(p: Pt, q: Pt) -> f32 {
    ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt()
}

/// A line through points; closed when it is filled.
#[derive(Clone, Debug, Default)]
pub struct Curve(Vec<Pt>);

/// A curve that starts at (x, y).
pub fn at(x: f32, y: f32) -> Curve {
    Curve(vec![(x, y)])
}

/// An arc of a circle from one angle to another, in degrees (0 points along +x; with y down, the
/// angles grow clockwise).
pub fn arc(centre: Pt, radius: f32, from: f32, to: f32) -> Curve {
    let n = ((to - from).abs() / 1.5).ceil().max(2.0) as usize;
    Curve(
        (0..=n)
            .map(|i| {
                let a = (from + (to - from) * i as f32 / n as f32).to_radians();
                (centre.0 + radius * a.cos(), centre.1 + radius * a.sin())
            })
            .collect(),
    )
}

/// An ellipse, turned `tilt` degrees about its centre.
pub fn ellipse(centre: Pt, rx: f32, ry: f32, tilt: f32) -> Curve {
    let n = 240;
    Curve(
        (0..n)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / n as f32;
                (centre.0 + rx * a.cos(), centre.1 + ry * a.sin())
            })
            .collect(),
    )
    .rotated(centre, tilt)
}

/// A circle.
pub fn circle(centre: Pt, r: f32) -> Curve {
    ellipse(centre, r, r, 0.0)
}

/// A polygon through the points given.
pub fn poly(points: &[Pt]) -> Curve {
    Curve(points.to_vec())
}

/// An engraved mark from `a` to `b`: pointed at both ends, `w` thick in the middle.
pub fn lens(a: Pt, b: Pt, w: f32) -> Curve {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let d = (dx * dx + dy * dy).sqrt().max(1e-3);
    // A cubic whose two control points stand h off the chord bulges 0.75 h.
    let (nx, ny) = (-dy / d * w / 1.5, dx / d * w / 1.5);
    let (p, q) = ((a.0 + dx / 3.0, a.1 + dy / 3.0), (a.0 + dx * 2.0 / 3.0, a.1 + dy * 2.0 / 3.0));
    at(a.0, a.1).c(p.0 + nx, p.1 + ny, q.0 + nx, q.1 + ny, b.0, b.1).c(q.0 - nx, q.1 - ny, p.0 - nx, p.1 - ny, a.0, a.1)
}

impl Curve {
    fn end(&self) -> Pt {
        self.0.last().copied().unwrap_or((0.0, 0.0))
    }

    /// A straight line on to (x, y).
    pub fn l(mut self, x: f32, y: f32) -> Curve {
        self.0.push((x, y));
        self
    }

    /// A cubic Bézier on to (x, y) with the control points (x1, y1) and (x2, y2).
    pub fn c(mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) -> Curve {
        let p0 = self.end();
        let n = (dist(p0, (x1, y1)) + dist((x1, y1), (x2, y2)) + dist((x2, y2), (x, y))).ceil().clamp(8.0, 160.0) as usize;
        self.0.extend(bezier(p0, (x1, y1), (x2, y2), (x, y), n));
        self
    }

    /// The same curve turned `degrees` about `centre`.
    pub fn rotated(&self, centre: Pt, degrees: f32) -> Curve {
        let (s, c) = degrees.to_radians().sin_cos();
        Curve(
            self.0
                .iter()
                .map(|&(x, y)| {
                    let (dx, dy) = (x - centre.0, y - centre.1);
                    (centre.0 + dx * c - dy * s, centre.1 + dx * s + dy * c)
                })
                .collect(),
        )
    }

    /// The same curve with each point moved by `f`.
    pub fn mapped(&self, f: impl Fn(Pt) -> Pt) -> Curve {
        Curve(self.0.iter().map(|&p| f(p)).collect())
    }

    /// How long it is.
    pub fn length(&self) -> f32 {
        self.0.windows(2).map(|w| if let [p, q] = w { dist(*p, *q) } else { 0.0 }).sum()
    }

    /// The stretch of it between two fractions of its length.
    pub fn part(&self, from: f32, to: f32) -> Curve {
        let total = self.length();
        let (a, b) = (from.clamp(0.0, 1.0) * total, to.clamp(0.0, 1.0) * total);
        let mut out = Vec::new();
        let mut run = 0.0;
        for w in self.0.windows(2) {
            let [p, q] = w else { continue };
            let d = dist(*p, *q);
            if d > 0.0 && run + d >= a && run <= b {
                if out.is_empty() {
                    out.push(lerp(*p, *q, ((a - run) / d).clamp(0.0, 1.0)));
                }
                out.push(lerp(*p, *q, ((b - run) / d).clamp(0.0, 1.0)));
            }
            run += d;
        }
        Curve(out)
    }

    /// The point a fraction of the way along it.
    pub fn point(&self, t: f32) -> Pt {
        self.part(0.0, t).end()
    }

    fn path(&self, close: bool) -> Option<Path> {
        let mut points = self.0.iter();
        let &(x, y) = points.next()?;
        let mut pb = PathBuilder::new();
        pb.move_to(x, y);
        for &(x, y) in points {
            pb.line_to(x, y);
        }
        if close {
            pb.close();
        }
        pb.finish()
    }
}

/// A pen with round ends and joints.
fn pen(width: f32) -> Stroke {
    Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() }
}

fn paint(part: Part) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(part.colour());
    paint.anti_alias = true;
    paint
}

/// The drawing surface: a square of pixels over the 1000-unit square.
pub struct Canvas {
    pix: Pixmap,
    ts: Transform,
}

impl Canvas {
    /// A canvas `side` pixels a side, all field.
    pub fn new(side: u32) -> Result<Canvas, String> {
        let mut pix = Pixmap::new(side, side).ok_or_else(|| format!("cannot make a {side} px canvas"))?;
        pix.fill(Part::Field.colour());
        let s = side as f32 / 1000.0;
        Ok(Canvas { pix, ts: Transform::from_scale(s, s) })
    }

    pub fn side(&self) -> u32 {
        self.pix.width()
    }

    /// Fills a closed shape, where `clip` lets it.
    pub fn fill(&mut self, shape: &Curve, part: Part, clip: Option<&Mask>) {
        if let Some(path) = shape.path(true) {
            self.pix.fill_path(&path, &paint(part), FillRule::Winding, self.ts, clip);
        }
    }

    /// Draws a line `width` units wide, where `clip` lets it.
    pub fn stroke(&mut self, line: &Curve, width: f32, part: Part, clip: Option<&Mask>) {
        if let Some(path) = line.path(false) {
            self.pix.stroke_path(&path, &paint(part), &pen(width), self.ts, clip);
        }
    }

    /// Parallel ink lines `gap` units apart running `dir` (a unit vector) across the square, where
    /// `clip` lets them.
    pub fn hatch(&mut self, dir: Pt, gap: f32, width: f32, clip: &Mask) {
        let normal = (-dir.1, dir.0);
        let mut k = -720.0;
        while k < 720.0 {
            let base = (500.0 + normal.0 * k, 500.0 + normal.1 * k);
            let line = at(base.0 - dir.0 * 720.0, base.1 - dir.1 * 720.0).l(base.0 + dir.0 * 720.0, base.1 + dir.1 * 720.0);
            self.stroke(&line, width, Part::Ink, Some(clip));
            k += gap;
        }
    }

    /// Paints everywhere a mask lets it.
    pub fn cover(&mut self, mask: &Mask, part: Part) {
        let side = self.side() as f32;
        if let Some(rect) = Rect::from_xywh(0.0, 0.0, side, side) {
            self.pix.fill_rect(rect, &paint(part), Transform::identity(), Some(mask));
        }
    }

    fn blank(&self) -> Result<Mask, String> {
        Mask::new(self.side(), self.side()).ok_or_else(|| "cannot make a mask".to_string())
    }

    /// A mask of closed shapes (all of them).
    pub fn mask(&self, shapes: &[&Curve]) -> Result<Mask, String> {
        let mut mask = self.blank()?;
        for path in shapes.iter().filter_map(|s| s.path(true)) {
            mask.fill_path(&path, FillRule::Winding, true, self.ts);
        }
        Ok(mask)
    }

    /// Each pixel as the part that covers most of it, row by row.
    pub fn parts(&self) -> Vec<Part> {
        self.pix
            .pixels()
            .iter()
            .map(|px| {
                let (r, g, b) = (px.red(), px.green(), px.blue());
                if b >= r && b >= g {
                    Part::Ink
                } else if g >= r {
                    Part::Paper
                } else {
                    Part::Field
                }
            })
            .collect()
    }
}

/// Where both masks let through.
pub fn both(a: &Mask, b: &Mask) -> Mask {
    let mut out = a.clone();
    for (o, m) in out.data_mut().iter_mut().zip(b.data()) {
        *o = ((u16::from(*o) * u16::from(*m) + 127) / 255) as u8;
    }
    out
}

/// Where the first mask lets through and the second doesn't.
pub fn without(a: &Mask, b: &Mask) -> Mask {
    let mut out = a.clone();
    for (o, m) in out.data_mut().iter_mut().zip(b.data()) {
        *o = ((u16::from(*o) * u16::from(255 - *m) + 127) / 255) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_go_where_they_are_told() {
        let line = at(0.0, 0.0).l(100.0, 0.0).l(100.0, 100.0);
        assert!((line.length() - 200.0).abs() < 1e-3);
        let (x, y) = line.point(0.25);
        assert!((x - 50.0).abs() < 1e-3 && y.abs() < 1e-3);
        let (x, y) = line.part(0.5, 0.75).end();
        assert!((x - 100.0).abs() < 1e-3 && (y - 50.0).abs() < 1e-3);
        let (x, y) = at(0.0, 0.0).c(0.0, 50.0, 50.0, 100.0, 100.0, 100.0).end();
        assert!((x - 100.0).abs() < 1e-3 && (y - 100.0).abs() < 1e-3);
        let (x, y) = at(10.0, 0.0).rotated((0.0, 0.0), 90.0).end();
        assert!(x.abs() < 1e-3 && (y - 10.0).abs() < 1e-3);
    }

    #[test]
    fn the_canvas_knows_three_parts() {
        let mut canvas = Canvas::new(100).unwrap();
        canvas.fill(&circle((500.0, 500.0), 300.0), Part::Paper, None);
        let inner = canvas.mask(&[&circle((500.0, 500.0), 150.0)]).unwrap();
        let left = canvas.mask(&[&poly(&[(0.0, 0.0), (500.0, 0.0), (500.0, 1000.0), (0.0, 1000.0)])]).unwrap();
        canvas.cover(&without(&inner, &left), Part::Ink);
        canvas.stroke(&at(0.0, 950.0).l(1000.0, 950.0), 40.0, Part::Ink, Some(&both(&left, &left)));
        let parts = canvas.parts();
        let at = |x: usize, y: usize| parts[y * 100 + x];
        assert_eq!(at(2, 2), Part::Field);
        assert_eq!(at(30, 50), Part::Paper);
        assert_eq!(at(40, 50), Part::Paper, "the left of the inner circle is cut away");
        assert_eq!(at(60, 50), Part::Ink);
        assert_eq!(at(20, 95), Part::Ink);
        assert_eq!(at(80, 95), Part::Field, "the line is clipped to the left half");
    }
}
