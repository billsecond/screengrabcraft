//! A part of the picture chosen to work on: a box, an ellipse, a freehand outline, or the pixels
//! of a colour (the magic wand), turned inside out when asked; and a stroke rubbed out with the
//! eraser. Each is worked out as a mask over the box round it, so moving, copying, deleting or
//! cropping takes just that shape, with soft edges.

use screengrabcraft_model::{Color, Pt, Raster, RasterError, Rect};
use serde::{Deserialize, Serialize};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Stroke, Transform};

use crate::canvas;

/// The most points a freehand outline or an eraser stroke keeps.
pub const MAX_OUTLINE: usize = 20_000;

fn yes() -> bool {
    true
}

fn tolerance() -> u8 {
    32
}

/// What shape the chosen part has. A box and an ellipse fill the area they are given; a
/// freehand outline goes through its points; the wand takes the pixels near the colour at its
/// seed (joined to it, or anywhere).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "camelCase")]
pub enum AreaShape {
    #[default]
    Rect,
    Ellipse,
    Freehand {
        points: Vec<Pt>,
    },
    Wand {
        seed: Pt,
        #[serde(default = "tolerance")]
        tolerance: u8,
        #[serde(default = "yes")]
        contiguous: bool,
    },
}

impl AreaShape {
    /// The same with every point finite and the outline bounded.
    pub fn sanitized(mut self) -> AreaShape {
        match &mut self {
            AreaShape::Freehand { points } => {
                points.truncate(MAX_OUTLINE);
                for p in points.iter_mut() {
                    *p = p.finite();
                }
            }
            AreaShape::Wand { seed, .. } => *seed = seed.finite(),
            AreaShape::Rect | AreaShape::Ellipse => {}
        }
        self
    }

    /// Moved by `d` (when the picture grows at its left or top).
    pub fn translate(&mut self, d: Pt) {
        match self {
            AreaShape::Freehand { points } => {
                for p in points.iter_mut() {
                    *p = p.add(d).finite();
                }
            }
            AreaShape::Wand { seed, .. } => *seed = seed.add(d).finite(),
            AreaShape::Rect | AreaShape::Ellipse => {}
        }
    }
}

/// The chosen part: the box round it on the picture, in whole pixels, and how much of each pixel
/// in that box is in it (0 to 255, row by row).
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub mask: Vec<u8>,
}

impl Region {
    /// All of a box.
    pub fn whole(x: u32, y: u32, w: u32, h: u32) -> Option<Region> {
        let n = screengrabcraft_model::raster::checked_len(w, h).ok()? / 4;
        Some(Region { x, y, w, h, mask: vec![255; n] })
    }

    /// How much of the picture's pixel `(px, py)` is in it.
    pub fn coverage(&self, px: u32, py: u32) -> u8 {
        if px < self.x || py < self.y || px >= self.x.saturating_add(self.w) || py >= self.y.saturating_add(self.h) {
            return 0;
        }
        let i = (py - self.y) as usize * self.w as usize + (px - self.x) as usize;
        self.mask.get(i).copied().unwrap_or(0)
    }

    /// Whether no pixel is in it.
    pub fn is_empty(&self) -> bool {
        self.mask.iter().all(|m| *m == 0)
    }

    /// Whether it is all of its box.
    pub fn is_box(&self) -> bool {
        self.mask.iter().all(|m| *m == 255)
    }

    /// Its box on the picture.
    pub fn rect(&self) -> Rect {
        Rect::new(self.x as f32, self.y as f32, self.w as f32, self.h as f32)
    }

    /// Cut down to the box round what is in it (none when nothing is).
    fn tightened(self) -> Option<Region> {
        let (w, h) = (self.w as usize, self.h as usize);
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for (i, m) in self.mask.iter().enumerate() {
            if *m > 0 {
                let (x, y) = (i % w.max(1), i / w.max(1));
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
        if x0 > x1 || y0 > y1 {
            return None;
        }
        if (x0, y0, x1, y1) == (0, 0, w - 1, h - 1) {
            return Some(self);
        }
        let (nw, nh) = (x1 - x0 + 1, y1 - y0 + 1);
        let mut mask = Vec::with_capacity(nw * nh);
        for row in y0..=y1 {
            mask.extend_from_slice(self.mask.get(row * w + x0..row * w + x1 + 1)?);
        }
        Some(Region { x: self.x + x0 as u32, y: self.y + y0 as u32, w: nw as u32, h: nh as u32, mask })
    }
}

/// A mask the size of `w`×`h` with `path` filled in (anti-aliased), placed by `t`.
fn filled(w: u32, h: u32, path: &tiny_skia::Path, t: Transform) -> Option<Vec<u8>> {
    let mut mask = tiny_skia::Mask::new(w, h)?;
    mask.fill_path(path, FillRule::EvenOdd, true, t);
    Some(mask.data().to_vec())
}

/// The part of `picture` chosen: `shape` in `area` (a box or an ellipse; a freehand outline and
/// the wand bring their own), or everything else when `invert`. None when nothing of the picture
/// is in it.
pub fn region(picture: &Raster, area: Rect, shape: &AreaShape, invert: bool) -> Option<Region> {
    let (pw, ph) = (picture.width(), picture.height());
    let inside = match shape {
        AreaShape::Rect => {
            let (x, y, w, h) = area.finite().pixel_bounds(pw, ph)?;
            Region::whole(x, y, w, h)?
        }
        AreaShape::Ellipse => {
            let (x, y, w, h) = area.finite().pixel_bounds(pw, ph)?;
            let a = area.finite();
            let oval = PathBuilder::from_oval(tiny_skia::Rect::from_xywh(a.x - x as f32, a.y - y as f32, a.w.max(0.5), a.h.max(0.5))?)?;
            Region { x, y, w, h, mask: filled(w, h, &oval, Transform::identity())? }
        }
        AreaShape::Freehand { points } => {
            let pts: Vec<Pt> = points.iter().take(MAX_OUTLINE).map(|p| p.finite()).collect();
            if pts.len() < 3 {
                return None;
            }
            let (x, y, w, h) = Rect::bounding(&pts).inflate(1.0).pixel_bounds(pw, ph)?;
            let mut pb = PathBuilder::new();
            let (first, rest) = pts.split_first()?;
            pb.move_to(first.x - x as f32, first.y - y as f32);
            for p in rest {
                pb.line_to(p.x - x as f32, p.y - y as f32);
            }
            pb.close();
            Region { x, y, w, h, mask: filled(w, h, &pb.finish()?, Transform::identity())? }
        }
        AreaShape::Wand { seed, tolerance, contiguous } => wand(picture, *seed, *tolerance, *contiguous)?,
    };
    let chosen = if invert {
        let mut all = Region::whole(0, 0, pw, ph)?;
        for (i, m) in all.mask.iter_mut().enumerate() {
            let (px, py) = ((i % pw as usize) as u32, (i / pw as usize) as u32);
            *m = 255 - inside.coverage(px, py);
        }
        all
    } else {
        inside
    };
    chosen.tightened()
}

/// How far apart two colours are: the largest difference of a channel, alpha included; two
/// see-through pixels are the same.
fn distance(a: [u8; 4], b: [u8; 4]) -> u8 {
    if a[3] == 0 && b[3] == 0 {
        return 0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0)
}

/// The magic wand: the pixels within `tolerance` (0 to 255 a channel) of the colour at `seed`,
/// joined to it side by side (`contiguous`) or anywhere on the picture.
fn wand(picture: &Raster, seed: Pt, tolerance: u8, contiguous: bool) -> Option<Region> {
    let (pw, ph) = (picture.width(), picture.height());
    let (sx, sy) = (seed.x.floor(), seed.y.floor());
    if !(sx >= 0.0 && sy >= 0.0 && sx < pw as f32 && sy < ph as f32) {
        return None;
    }
    let (sx, sy) = (sx as usize, sy as usize);
    let px = picture.pixels();
    let at = |i: usize| -> [u8; 4] {
        match px.get(i * 4..i * 4 + 4) {
            Some([r, g, b, a]) => [*r, *g, *b, *a],
            _ => [0; 4],
        }
    };
    let colour = at(sy * pw as usize + sx);
    let n = (pw as usize).checked_mul(ph as usize)?;
    let mut taken = vec![0u8; n];
    if contiguous {
        let near = |i: usize| distance(at(i), colour) <= tolerance;
        crate::matte::flood(pw as usize, ph as usize, &[(sx, sy)], &near, &mut taken);
    } else {
        for (i, t) in taken.iter_mut().enumerate() {
            if distance(at(i), colour) <= tolerance {
                *t = 1;
            }
        }
    }
    let mask: Vec<u8> = taken.iter().map(|t| if *t != 0 { 255 } else { 0 }).collect();
    Region { x: 0, y: 0, w: pw, h: ph, mask }.tightened()
}

/// The picture with what is in `reg` made see-through (partly, at its soft edges).
pub fn erase(r: &Raster, reg: &Region) -> Raster {
    paint(r, reg, Color::TRANSPARENT)
}

/// The picture with what is in `reg` replaced by `colour` (blended at its soft edges); a
/// see-through colour clears it.
pub fn paint(r: &Raster, reg: &Region, colour: Color) -> Raster {
    let mut out = r.clone();
    let stride = r.width() as usize;
    let new = [colour.r, colour.g, colour.b, colour.a];
    let px = out.pixels_mut();
    for row in 0..reg.h as usize {
        for col in 0..reg.w as usize {
            let m = reg.mask.get(row * reg.w as usize + col).copied().unwrap_or(0);
            if m == 0 {
                continue;
            }
            let i = ((reg.y as usize + row) * stride + reg.x as usize + col) * 4;
            let Some(p) = px.get_mut(i..i + 4) else { continue };
            let k = f32::from(m) / 255.0;
            if colour.a == 0 {
                p[3] = (f32::from(p[3]) * (1.0 - k)).round() as u8;
            } else {
                for (c, n) in p.iter_mut().zip(new) {
                    *c = (f32::from(*c) + (f32::from(n) - f32::from(*c)) * k).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    out
}

/// What is in `reg`, on its own: its box of the picture, see-through outside the shape.
pub fn piece(r: &Raster, reg: &Region) -> Option<Raster> {
    let mut out = r.crop(reg.x, reg.y, reg.w, reg.h)?;
    if reg.is_box() {
        return Some(out);
    }
    for (p, m) in out.pixels_mut().as_chunks_mut::<4>().0.iter_mut().zip(&reg.mask) {
        p[3] = (u16::from(p[3]) * u16::from(*m) / 255) as u8;
    }
    Some(out)
}

/// `done` (the region's box of the picture, changed) put back into `r` where the region is,
/// blended at its soft edges: an effect kept to a shape.
pub fn blend_in(r: &Raster, done: &Raster, reg: &Region) -> Raster {
    let mut out = r.clone();
    let stride = r.width() as usize;
    let src = done.pixels();
    let px = out.pixels_mut();
    for row in 0..reg.h as usize {
        for col in 0..reg.w as usize {
            let m = reg.mask.get(row * reg.w as usize + col).copied().unwrap_or(0);
            if m == 0 {
                continue;
            }
            let i = ((reg.y as usize + row) * stride + reg.x as usize + col) * 4;
            let j = (row * done.width() as usize + col) * 4;
            let (Some(p), Some(s)) = (px.get_mut(i..i + 4), src.get(j..j + 4)) else { continue };
            let k = f32::from(m) / 255.0;
            for (c, n) in p.iter_mut().zip(s) {
                *c = (f32::from(*c) + (f32::from(*n) - f32::from(*c)) * k).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// The picture with what is in `reg` filled from round it: from the pixels just outside it
/// inwards, a ring at a time, each pixel the average of its neighbours already known (a flat
/// background closes over seamlessly; a gradient is carried in). Blended at its soft edges. With
/// nothing round it (all of the picture), it is made see-through.
pub fn fill_around(r: &Raster, reg: &Region) -> Raster {
    let (pw, ph) = (r.width() as usize, r.height() as usize);
    // The box worked in: the region's, with a pixel round it to fill from.
    let x0 = (reg.x as usize).saturating_sub(1);
    let y0 = (reg.y as usize).saturating_sub(1);
    let x1 = (reg.x as usize + reg.w as usize + 1).min(pw);
    let y1 = (reg.y as usize + reg.h as usize + 1).min(ph);
    let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    if w == 0 || h == 0 {
        return r.clone();
    }
    let src = r.pixels();
    let mut colour = vec![[0f32; 4]; w * h];
    let mut known = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let (px, py) = ((x0 + x) as u32, (y0 + y) as u32);
            let i = y * w + x;
            if reg.coverage(px, py) == 0 {
                let j = ((y0 + y) * pw + x0 + x) * 4;
                if let (Some(c), Some(p)) = (colour.get_mut(i), src.get(j..j + 4)) {
                    *c = [f32::from(p[0]), f32::from(p[1]), f32::from(p[2]), f32::from(p[3])];
                }
                if let Some(k) = known.get_mut(i) {
                    *k = true;
                }
            }
        }
    }
    if !known.iter().any(|k| *k) {
        return erase(r, reg);
    }
    let near: [(isize, isize, f32); 8] =
        [(-1, 0, 2.0), (1, 0, 2.0), (0, -1, 2.0), (0, 1, 2.0), (-1, -1, 1.0), (1, -1, 1.0), (-1, 1, 1.0), (1, 1, 1.0)];
    let neighbour = |i: usize, dx: isize, dy: isize| -> Option<usize> {
        let (x, y) = ((i % w) as isize + dx, (i / w) as isize + dy);
        (x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h).then(|| y as usize * w + x as usize)
    };
    // The first ring: the pixels to fill that touch one known.
    let mut ring: Vec<usize> = (0..w * h)
        .filter(|&i| {
            !known.get(i).copied().unwrap_or(true)
                && near.iter().any(|(dx, dy, _)| neighbour(i, *dx, *dy).is_some_and(|n| known.get(n).copied().unwrap_or(false)))
        })
        .collect();
    let mut queued = vec![false; w * h];
    for &i in &ring {
        if let Some(q) = queued.get_mut(i) {
            *q = true;
        }
    }
    while !ring.is_empty() {
        let mut done = Vec::with_capacity(ring.len());
        for &i in &ring {
            let (mut sum, mut weight) = ([0f32; 4], 0f32);
            for (dx, dy, wt) in near {
                if let Some(n) = neighbour(i, dx, dy)
                    && known.get(n).copied().unwrap_or(false)
                    && let Some(c) = colour.get(n)
                {
                    for (s, v) in sum.iter_mut().zip(c) {
                        *s += v * wt;
                    }
                    weight += wt;
                }
            }
            if weight > 0.0 {
                done.push((i, sum.map(|s| s / weight)));
            }
        }
        let mut next = Vec::new();
        for (i, c) in done {
            if let Some(slot) = colour.get_mut(i) {
                *slot = c;
            }
            if let Some(k) = known.get_mut(i) {
                *k = true;
            }
        }
        for &i in &ring {
            for (dx, dy, _) in near {
                if let Some(n) = neighbour(i, dx, dy)
                    && !known.get(n).copied().unwrap_or(true)
                    && !queued.get(n).copied().unwrap_or(true)
                {
                    if let Some(q) = queued.get_mut(n) {
                        *q = true;
                    }
                    next.push(n);
                }
            }
        }
        ring = next;
    }
    // The region's box of the picture, filled, blended back in at the region's soft edges.
    let Some(mut filled) = r.crop(reg.x, reg.y, reg.w, reg.h) else { return r.clone() };
    let fw = reg.w as usize;
    for (k, p) in filled.pixels_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (reg.x as usize + k % fw - x0, reg.y as usize + k / fw - y0);
        if let Some(c) = colour.get(y * w + x) {
            *p = c.map(|v| v.round().clamp(0.0, 255.0) as u8);
        }
    }
    blend_in(r, &filled, reg)
}

/// The pixels an eraser `width` pixels wide passes over along `points`, on a picture of
/// `pw`×`ph`. None when it passes over none.
pub fn stroke(pw: u32, ph: u32, points: &[Pt], width: f32) -> Result<Option<Region>, RasterError> {
    let width = if width.is_finite() { width.clamp(1.0, 400.0) } else { 20.0 };
    let pts: Vec<Pt> = points.iter().take(MAX_OUTLINE).map(|p| p.finite()).collect();
    let Some(first) = pts.first() else { return Ok(None) };
    let Some((x, y, w, h)) = Rect::bounding(&pts).inflate(width / 2.0 + 1.0).pixel_bounds(pw, ph) else { return Ok(None) };
    let mut pix = canvas::blank(w, h).ok_or(RasterError::Empty(w, h))?;
    let mut pb = PathBuilder::new();
    pb.move_to(first.x - x as f32, first.y - y as f32);
    if pts.len() == 1 {
        pb.line_to(first.x - x as f32 + 0.01, first.y - y as f32);
    }
    for p in pts.iter().skip(1) {
        pb.line_to(p.x - x as f32, p.y - y as f32);
    }
    let Some(path) = pb.finish() else { return Ok(None) };
    let mut white = Paint { anti_alias: true, ..Paint::default() };
    white.set_color(tiny_skia::Color::WHITE);
    let s = Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    pix.stroke_path(&path, &white, &s, Transform::identity(), None);
    let mask: Vec<u8> = pix.data().as_chunks::<4>().0.iter().map(|p| p[3]).collect();
    Ok(Region { x, y, w, h, mask }.tightened())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// White, with a red square in the middle and a red one in the corner.
    fn pic() -> Raster {
        let mut r = Raster::filled(40, 30, Color::WHITE).unwrap();
        for (x0, y0) in [(15, 10), (0, 0)] {
            for y in y0..y0 + 6 {
                for x in x0..x0 + 6 {
                    let i = (y * 40 + x) * 4;
                    r.pixels_mut()[i..i + 4].copy_from_slice(&[230, 20, 20, 255]);
                }
            }
        }
        r
    }

    #[test]
    fn a_box_an_ellipse_and_a_freehand_outline() {
        let r = pic();
        let b = region(&r, Rect::new(5.0, 5.0, 20.0, 10.0), &AreaShape::Rect, false).unwrap();
        assert_eq!((b.x, b.y, b.w, b.h, b.is_box()), (5, 5, 20, 10, true));
        let e = region(&r, Rect::new(0.0, 0.0, 40.0, 30.0), &AreaShape::Ellipse, false).unwrap();
        assert_eq!((e.coverage(20, 15), e.coverage(0, 0), e.coverage(39, 29)), (255, 0, 0));
        let tri = AreaShape::Freehand { points: vec![Pt::new(0.0, 0.0), Pt::new(30.0, 0.0), Pt::new(0.0, 30.0)] };
        let f = region(&r, Rect::default(), &tri, false).unwrap();
        assert_eq!((f.coverage(3, 3), f.coverage(28, 28)), (255, 0));
        // Off the picture, or too few points: nothing.
        assert!(region(&r, Rect::new(100.0, 0.0, 5.0, 5.0), &AreaShape::Rect, false).is_none());
        assert!(region(&r, Rect::default(), &AreaShape::Freehand { points: vec![Pt::ZERO, Pt::new(f32::NAN, 1.0)] }, false).is_none());
    }

    #[test]
    fn the_wand_takes_a_colour_joined_or_anywhere_and_invert_takes_the_rest() {
        let r = pic();
        let seed = Pt::new(17.0, 12.0);
        let joined = region(&r, Rect::default(), &AreaShape::Wand { seed, tolerance: 10, contiguous: true }, false).unwrap();
        assert_eq!((joined.x, joined.y, joined.w, joined.h), (15, 10, 6, 6), "the middle square only");
        let anywhere = region(&r, Rect::default(), &AreaShape::Wand { seed, tolerance: 10, contiguous: false }, false).unwrap();
        assert_eq!((anywhere.x, anywhere.y, anywhere.w, anywhere.h), (0, 0, 21, 16), "both squares");
        let rest = region(&r, Rect::default(), &AreaShape::Wand { seed, tolerance: 10, contiguous: true }, true).unwrap();
        assert_eq!((rest.coverage(17, 12), rest.coverage(30, 20), rest.w, rest.h), (0, 255, 40, 30));
        assert!(region(&r, Rect::default(), &AreaShape::Wand { seed: Pt::new(-1.0, 5.0), tolerance: 10, contiguous: true }, false).is_none());
        // Everything inverted is nothing.
        assert!(region(&r, Rect::new(0.0, 0.0, 40.0, 30.0), &AreaShape::Rect, true).is_none());
    }

    #[test]
    fn a_hole_is_filled_from_round_it() {
        let r = pic();
        // The middle square, taken out: the white round it closes over it.
        let sq = region(&r, Rect::new(14.0, 9.0, 8.0, 8.0), &AreaShape::Rect, false).unwrap();
        let filled = fill_around(&r, &sq);
        for (x, y) in [(15, 10), (17, 12), (20, 15)] {
            assert_eq!(filled.pixel(x, y), Some(Color::WHITE), "{x},{y}");
        }
        // The rest is as it was (the corner square too).
        assert_eq!(filled.pixel(2, 2), r.pixel(2, 2));
        // Between two colours, a hole takes some of each.
        let mut halves = Raster::filled(20, 10, Color::rgb(0, 0, 0)).unwrap();
        for (i, p) in halves.pixels_mut().chunks_mut(4).enumerate() {
            if i % 20 >= 10 {
                p.copy_from_slice(&[200, 200, 200, 255]);
            }
        }
        let mid = region(&halves, Rect::new(6.0, 2.0, 8.0, 6.0), &AreaShape::Ellipse, false).unwrap();
        let f = fill_around(&halves, &mid);
        let (left, right) = (f.pixel(7, 5).unwrap(), f.pixel(12, 5).unwrap());
        assert!(left.r < right.r, "{left:?} {right:?}");
        // All of the picture: nothing round it, so it is made see-through.
        let all = region(&r, Rect::new(0.0, 0.0, 40.0, 30.0), &AreaShape::Rect, false).unwrap();
        assert_eq!(fill_around(&r, &all).pixel(5, 5).map(|c| c.a), Some(0));
    }

    #[test]
    fn erase_paint_piece_and_blend_keep_to_the_shape() {
        let r = pic();
        let e = region(&r, Rect::new(10.0, 5.0, 20.0, 20.0), &AreaShape::Ellipse, false).unwrap();
        let gone = erase(&r, &e);
        assert_eq!(gone.pixel(20, 15).map(|c| c.a), Some(0));
        assert_eq!(gone.pixel(10, 5), Some(Color::WHITE), "the box's corner is outside the ellipse");
        let blue = paint(&r, &e, Color::rgb(0, 0, 255));
        assert_eq!(blue.pixel(20, 15), Some(Color::rgb(0, 0, 255)));
        let p = piece(&r, &e).unwrap();
        assert_eq!((p.width(), p.height(), p.pixel(0, 0).map(|c| c.a)), (20, 20, Some(0)));
        let dark = Raster::filled(20, 20, Color::BLACK).unwrap();
        let mixed = blend_in(&r, &dark, &e);
        assert_eq!((mixed.pixel(20, 15), mixed.pixel(10, 5)), (Some(Color::BLACK), Some(Color::WHITE)));
    }

    #[test]
    fn an_eraser_stroke_covers_its_width_along_its_points() {
        let s = stroke(40, 30, &[Pt::new(5.0, 15.0), Pt::new(35.0, 15.0)], 6.0).unwrap().unwrap();
        assert_eq!((s.coverage(20, 15), s.coverage(20, 5)), (255, 0));
        assert!(s.coverage(20, 17) > 200);
        assert!(stroke(40, 30, &[], 6.0).unwrap().is_none());
        assert!(stroke(40, 30, &[Pt::new(500.0, 500.0)], 6.0).unwrap().is_none());
        assert!(stroke(40, 30, &[Pt::new(10.0, 10.0)], f32::NAN).unwrap().is_some(), "a dab");
    }
}
