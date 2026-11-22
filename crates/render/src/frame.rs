//! What happens to the picture's edges and its shape: wavy and faded edges, a bevel, a tilt in
//! perspective, a page curl at a corner and a reflection under it (the torn edge, the border, the
//! drop shadow and the round corners are in [`crate::effects`]).

use screengrabcraft_model::{Color, Pt, Raster, RasterError};
use tiny_skia::{BlendMode, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, PixmapPaint, Point, SpreadMode, Transform};

use crate::canvas;
use crate::effects::Edges;

fn bad(r: &Raster) -> RasterError {
    RasterError::Empty(r.width(), r.height())
}

fn bounded(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { default }
}

/// Every pixel's alpha multiplied by what `keep` says (0 to 1) for its middle.
fn fade_by(r: &Raster, keep: impl Fn(f32, f32) -> f32) -> Raster {
    let w = r.width() as usize;
    let mut out = r.clone();
    for (i, px) in out.pixels_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = ((i % w.max(1)) as f32 + 0.5, (i / w.max(1)) as f32 + 0.5);
        let k = keep(x, y).clamp(0.0, 1.0);
        px[3] = (f32::from(px[3]) * k).round() as u8;
    }
    out
}

/// The picture with gently waving edges on the chosen sides: waves `depth` pixels deep (2 to
/// 100) and `length` pixels from crest to crest (8 to 400).
pub fn wavy_edge(r: &Raster, edges: Edges, depth: f32, length: f32) -> Result<Raster, RasterError> {
    let (w, h) = (r.width() as f32, r.height() as f32);
    let d = bounded(depth, 2.0, 100.0, 10.0).min(w / 4.0).min(h / 4.0).max(0.5);
    let wave = bounded(length, 8.0, 400.0, 40.0);
    let mut pb = PathBuilder::new();
    let side = |pb: &mut PathBuilder, a: (f32, f32), b: (f32, f32), inward: (f32, f32), waving: bool| {
        if !waving {
            pb.line_to(b.0, b.1);
            return;
        }
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        // A whole number of waves, so every corner meets the next side at the same depth.
        let waves = (len / wave).round().max(1.0);
        let n = (len / 2.0).ceil().clamp(8.0, 20_000.0) as usize;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let off = d * 0.5 * (1.0 - (t * waves * std::f32::consts::TAU).cos());
            pb.line_to(a.0 + (b.0 - a.0) * t + inward.0 * off, a.1 + (b.1 - a.1) * t + inward.1 * off);
        }
    };
    pb.move_to(0.0, 0.0);
    side(&mut pb, (0.0, 0.0), (w, 0.0), (0.0, 1.0), edges.top);
    side(&mut pb, (w, 0.0), (w, h), (-1.0, 0.0), edges.right);
    side(&mut pb, (w, h), (0.0, h), (0.0, -1.0), edges.bottom);
    side(&mut pb, (0.0, h), (0.0, 0.0), (1.0, 0.0), edges.left);
    pb.close();
    let path = pb.finish().ok_or(bad(r))?;
    let mut mask = tiny_skia::Mask::new(r.width(), r.height()).ok_or(bad(r))?;
    mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
    let mut out = r.clone();
    for (px, a) in out.pixels_mut().as_chunks_mut::<4>().0.iter_mut().zip(mask.data().iter()) {
        px[3] = (u16::from(px[3]) * u16::from(*a) / 255) as u8;
    }
    Ok(out)
}

/// The picture fading away to see-through over `width` pixels (2 to 400) at the chosen sides.
pub fn faded_edge(r: &Raster, edges: Edges, width: f32) -> Raster {
    let (w, h) = (r.width() as f32, r.height() as f32);
    let fade = bounded(width, 2.0, 400.0, 30.0).min(w / 2.0).min(h / 2.0).max(0.5);
    let smooth = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    fade_by(r, |x, y| {
        let mut k = 1.0f32;
        for (on, dist) in [(edges.top, y), (edges.bottom, h - y), (edges.left, x), (edges.right, w - x)] {
            if on {
                k = k.min(smooth(dist / fade));
            }
        }
        k
    })
}

/// A raised edge `width` pixels wide (1 to 100): lit along the top and the left, shaded along
/// the bottom and the right, `strength` (0 to 100) strong.
pub fn bevel(r: &Raster, width: f32, strength: f32) -> Raster {
    let (w, h) = (r.width() as f32, r.height() as f32);
    let band = bounded(width, 1.0, 100.0, 8.0).min(w / 2.0).min(h / 2.0).max(0.5);
    let k = bounded(strength, 0.0, 100.0, 50.0) / 100.0 * 0.8;
    let stride = r.width() as usize;
    let mut out = r.clone();
    for (i, px) in out.pixels_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = ((i % stride.max(1)) as f32 + 0.5, (i / stride.max(1)) as f32 + 0.5);
        // The nearest side decides: light from the top left.
        let (lit, dist) =
            [(true, y), (true, x), (false, h - y), (false, w - x)].into_iter().fold((true, f32::MAX), |best, s| if s.1 < best.1 { s } else { best });
        if dist >= band {
            continue;
        }
        let amount = (1.0 - dist / band) * k;
        let to = if lit { 255.0 } else { 0.0 };
        for c in px.iter_mut().take(3) {
            *c = (f32::from(*c) + (to - f32::from(*c)) * amount).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// A projective map: a 3×3 matrix taking (x, y, 1) somewhere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection([f32; 9]);

impl Projection {
    /// The map taking the unit square's corners (0,0), (1,0), (1,1), (0,1) to `q`'s.
    fn square_to(q: [Pt; 4]) -> Option<Projection> {
        let [p0, p1, p2, p3] = q;
        let (dx1, dx2, dx3) = (p1.x - p2.x, p3.x - p2.x, p0.x - p1.x + p2.x - p3.x);
        let (dy1, dy2, dy3) = (p1.y - p2.y, p3.y - p2.y, p0.y - p1.y + p2.y - p3.y);
        let (g, h) = if dx3.abs() < 1e-6 && dy3.abs() < 1e-6 {
            (0.0, 0.0)
        } else {
            let den = dx1 * dy2 - dx2 * dy1;
            if den.abs() < 1e-9 {
                return None;
            }
            ((dx3 * dy2 - dx2 * dy3) / den, (dx1 * dy3 - dx3 * dy1) / den)
        };
        let m = [p1.x - p0.x + g * p1.x, p3.x - p0.x + h * p3.x, p0.x, p1.y - p0.y + g * p1.y, p3.y - p0.y + h * p3.y, p0.y, g, h, 1.0];
        m.iter().all(|v| v.is_finite()).then_some(Projection(m))
    }

    /// Where it takes `p`.
    pub fn apply(&self, p: Pt) -> Pt {
        let m = &self.0;
        let w = m[6] * p.x + m[7] * p.y + m[8];
        if w.abs() < 1e-9 {
            return p;
        }
        Pt::new((m[0] * p.x + m[1] * p.y + m[2]) / w, (m[3] * p.x + m[4] * p.y + m[5]) / w).finite()
    }

    /// The map back.
    fn inverse(&self) -> Option<Projection> {
        let [a, b, c, d, e, f, g, h, i] = self.0;
        let (ca, cb, cc) = (e * i - f * h, -(d * i - f * g), d * h - e * g);
        let det = a * ca + b * cb + c * cc;
        if det.abs() < 1e-12 {
            return None;
        }
        let m = [ca, -(b * i - c * h), b * f - c * e, cb, a * i - c * g, -(a * f - c * d), cc, -(a * h - b * g), a * e - b * d].map(|v| v / det);
        m.iter().all(|v| v.is_finite()).then_some(Projection(m))
    }

    /// This map with the matrix `before` done to a point first.
    fn then_after(&self, before: [f32; 9]) -> Projection {
        let (a, b) = (&self.0, &before);
        let mut m = [0.0f32; 9];
        for (k, slot) in m.iter_mut().enumerate() {
            let (row, col) = (k / 3, k % 3);
            *slot = (0..3).map(|j| a.get(row * 3 + j).copied().unwrap_or(0.0) * b.get(j * 3 + col).copied().unwrap_or(0.0)).sum();
        }
        Projection(m)
    }
}

/// The colour at `(x, y)` of a premultiplied pixmap, between its pixels (none outside it).
fn sample(src: &Pixmap, x: f32, y: f32) -> Option<[f32; 4]> {
    let (w, h) = (src.width() as i64, src.height() as i64);
    let (fx, fy) = (x - 0.5, y - 0.5);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    if x0 < -1 || y0 < -1 || x0 >= w || y0 >= h {
        return None;
    }
    let data = src.data();
    let at = |xx: i64, yy: i64| -> [f32; 4] {
        let (xx, yy) = (xx.clamp(0, w - 1), yy.clamp(0, h - 1));
        let i = ((yy * w + xx) * 4) as usize;
        match data.get(i..i + 4) {
            Some([r, g, b, a]) => [f32::from(*r), f32::from(*g), f32::from(*b), f32::from(*a)],
            _ => [0.0; 4],
        }
    };
    let (p00, p10, p01, p11) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
    let mut out = [0.0f32; 4];
    for (c, o) in out.iter_mut().enumerate() {
        let top = p00[c] + (p10[c] - p00[c]) * tx;
        let bottom = p01[c] + (p11[c] - p01[c]) * tx;
        *o = top + (bottom - top) * ty;
    }
    Some(out)
}

/// Where the corners of a `w`×`h` picture go when it is tilted away: `horizontal` (-100 to 100)
/// takes its right side away (its left, below nought), `vertical` its bottom (its top). Top left,
/// top right, bottom right, bottom left, with the picture's box starting at 0, 0.
pub fn perspective_corners(w: f32, h: f32, horizontal: f32, vertical: f32) -> [Pt; 4] {
    let sx = bounded(horizontal, -100.0, 100.0, 0.0) / 100.0 * 0.6;
    let sy = bounded(vertical, -100.0, 100.0, 0.0) / 100.0 * 0.6;
    // In a unit square: the far side shorter, and nearer the other.
    let mut q = [Pt::new(0.0, 0.0), Pt::new(1.0, 0.0), Pt::new(1.0, 1.0), Pt::new(0.0, 1.0)];
    let (shrink_x, pull_x) = (sx.abs() / 2.0, sx.abs() * 0.35);
    let far_x: [usize; 2] = if sx >= 0.0 { [1, 2] } else { [0, 3] };
    for (k, i) in far_x.into_iter().enumerate() {
        if let Some(p) = q.get_mut(i) {
            p.y = if k == 0 { shrink_x } else { 1.0 - shrink_x };
            if sx >= 0.0 {
                p.x = 1.0 - pull_x;
            } else {
                p.x = pull_x;
            }
        }
    }
    let (shrink_y, pull_y) = (sy.abs() / 2.0, sy.abs() * 0.35);
    let far_y: [usize; 2] = if sy >= 0.0 { [3, 2] } else { [0, 1] };
    for (k, i) in far_y.into_iter().enumerate() {
        if let Some(p) = q.get_mut(i) {
            // Towards the middle of that side, as the corners there are.
            let mid = 0.5;
            p.x = if k == 0 { p.x + (mid - p.x) * shrink_y * 2.0 } else { p.x - (p.x - mid) * shrink_y * 2.0 };
            p.y = if sy >= 0.0 { p.y.min(1.0 - pull_y) } else { p.y.max(pull_y) };
        }
    }
    let (x0, y0) = (q.iter().map(|p| p.x).fold(f32::MAX, f32::min), q.iter().map(|p| p.y).fold(f32::MAX, f32::min));
    q.map(|p| Pt::new((p.x - x0) * w, (p.y - y0) * h))
}

/// The picture tilted away from the viewer (see [`perspective_corners`]), on a see-through
/// canvas just large enough for it, and the map taking a point of the old picture to the new.
pub fn perspective(r: &Raster, horizontal: f32, vertical: f32) -> Result<(Raster, Projection), RasterError> {
    let (w, h) = (r.width() as f32, r.height() as f32);
    let q = perspective_corners(w, h, horizontal, vertical);
    let ow = q.iter().map(|p| p.x).fold(0.0f32, f32::max).ceil().max(1.0) as u32;
    let oh = q.iter().map(|p| p.y).fold(0.0f32, f32::max).ceil().max(1.0) as u32;
    // Pixels → the unit square → the new corners.
    let unit = Projection::square_to(q).ok_or(bad(r))?;
    let forward = unit.then_after([1.0 / w, 0.0, 0.0, 0.0, 1.0 / h, 0.0, 0.0, 0.0, 1.0]);
    let back = forward.inverse().ok_or(bad(r))?;
    let src = canvas::to_pixmap(r).ok_or(bad(r))?;
    let mut out = canvas::blank(ow, oh).ok_or(bad(r))?;
    let stride = ow as usize;
    for (i, px) in out.data_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = ((i % stride) as f32 + 0.5, (i / stride) as f32 + 0.5);
        let p = back.apply(Pt::new(x, y));
        // A soft edge: a pixel half outside is half there.
        let inside = |v: f32, max: f32| (v + 0.5).min(max + 0.5 - v).clamp(0.0, 1.0);
        let cover = inside(p.x, w) * inside(p.y, h);
        if cover <= 0.0 {
            continue;
        }
        if let Some(c) = sample(&src, p.x.clamp(0.0, w), p.y.clamp(0.0, h)) {
            for (o, v) in px.iter_mut().zip(c) {
                *o = (v * cover).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Ok((canvas::from_pixmap(&out).ok_or(bad(r))?, forward))
}

/// A corner of the picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Corner {
    TopLeft,
    TopRight,
    #[default]
    BottomRight,
    BottomLeft,
}

impl Corner {
    pub const ALL: [Corner; 4] = [Corner::TopLeft, Corner::TopRight, Corner::BottomRight, Corner::BottomLeft];

    pub fn label(self) -> &'static str {
        match self {
            Corner::TopLeft => "Top left",
            Corner::TopRight => "Top right",
            Corner::BottomRight => "Bottom right",
            Corner::BottomLeft => "Bottom left",
        }
    }
}

/// The picture with a corner peeled back: the corner `size` of the way (5 to 60, in hundredths of
/// its shorter side) folded over, the page's back shaded, a soft shadow under it, and see-through
/// where the corner was.
pub fn page_curl(r: &Raster, corner: Corner, size: f32) -> Result<Raster, RasterError> {
    let (w, h) = (r.width() as f32, r.height() as f32);
    let s = (bounded(size, 5.0, 60.0, 20.0) / 100.0 * w.min(h)).max(2.0);
    let (k, ex, ey) = match corner {
        Corner::TopLeft => (Pt::new(0.0, 0.0), 1.0, 1.0),
        Corner::TopRight => (Pt::new(w, 0.0), -1.0, 1.0),
        Corner::BottomRight => (Pt::new(w, h), -1.0, -1.0),
        Corner::BottomLeft => (Pt::new(0.0, h), 1.0, -1.0),
    };
    let a = Pt::new(k.x + ex * s, k.y);
    let b = Pt::new(k.x, k.y + ey * s);
    // The corner folded over the line from a to b lands here.
    let tip = Pt::new(a.x + b.x - k.x, a.y + b.y - k.y);
    let mut pix = canvas::to_pixmap(r).ok_or(bad(r))?;
    // Where the corner was is gone.
    let mut cut = PathBuilder::new();
    cut.move_to(k.x, k.y);
    cut.line_to(a.x, a.y);
    cut.line_to(b.x, b.y);
    cut.close();
    let cut = cut.finish().ok_or(bad(r))?;
    let mut clear = Paint { blend_mode: BlendMode::Clear, anti_alias: true, ..Paint::default() };
    clear.set_color(tiny_skia::Color::BLACK);
    pix.fill_path(&cut, &clear, FillRule::Winding, Transform::identity(), None);
    // The folded page: its sides bowed a little, as paper curls.
    let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let bow = |from: Pt, to: Pt| {
        let m = from.lerp(to, 0.5);
        m.add(k.sub(mid).scale(0.12))
    };
    let (ca, cb) = (bow(a, tip), bow(b, tip));
    let mut flap = PathBuilder::new();
    flap.move_to(a.x, a.y);
    flap.quad_to(ca.x, ca.y, tip.x, tip.y);
    flap.quad_to(cb.x, cb.y, b.x, b.y);
    flap.close();
    let flap = flap.finish().ok_or(bad(r))?;
    // Its shadow, soft, on the page under it.
    if let Some(mut shadow) = canvas::blank(r.width(), r.height()) {
        let mut dark = Paint { anti_alias: true, ..Paint::default() };
        dark.set_color(tiny_skia::Color::from_rgba8(0, 0, 0, 110));
        shadow.fill_path(&flap, &dark, FillRule::Winding, Transform::from_translate(-ex * s * 0.04, -ey * s * 0.04), None);
        canvas::blur(&mut shadow, (s * 0.06).max(1.0));
        // Only on what is left of the page.
        shadow.fill_path(&cut, &clear, FillRule::Winding, Transform::identity(), None);
        pix.draw_pixmap(0, 0, shadow.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    }
    // The page's back: shaded along the fold, light at the tip.
    let stops = vec![
        GradientStop::new(0.0, tiny_skia::Color::from_rgba8(196, 196, 200, 255)),
        GradientStop::new(0.55, tiny_skia::Color::from_rgba8(246, 246, 248, 255)),
        GradientStop::new(1.0, tiny_skia::Color::from_rgba8(222, 222, 226, 255)),
    ];
    let mut paper = Paint { anti_alias: true, ..Paint::default() };
    match LinearGradient::new(Point::from_xy(mid.x, mid.y), Point::from_xy(tip.x, tip.y), stops, SpreadMode::Pad, Transform::identity()) {
        Some(shader) => paper.shader = shader,
        None => paper.set_color(tiny_skia::Color::from_rgba8(236, 236, 240, 255)),
    }
    pix.fill_path(&flap, &paper, FillRule::Winding, Transform::identity(), None);
    let mut edge = Paint { anti_alias: true, ..Paint::default() };
    edge.set_color(tiny_skia::Color::from_rgba8(0, 0, 0, 60));
    pix.stroke_path(&flap, &edge, &tiny_skia::Stroke { width: 1.0, ..tiny_skia::Stroke::default() }, Transform::identity(), None);
    canvas::from_pixmap(&pix).ok_or(bad(r))
}

/// The picture standing on a shiny floor: below it, after `gap` pixels (0 to 200), its bottom
/// `height` hundredths (5 to 100) mirrored, from `opacity` (0 to 100) fading away to nothing. The
/// picture grows downwards to hold it.
pub fn reflection(r: &Raster, height: f32, opacity: f32, gap: f32) -> Result<Raster, RasterError> {
    let (w, h) = (r.width(), r.height());
    let rh = ((bounded(height, 5.0, 100.0, 35.0) / 100.0 * h as f32).round() as u32).clamp(1, h);
    let gap = bounded(gap, 0.0, 200.0, 0.0).round() as u32;
    let start = bounded(opacity, 0.0, 100.0, 40.0) / 100.0;
    let total = h.checked_add(gap).and_then(|v| v.checked_add(rh)).ok_or(RasterError::TooLarge(w, h))?;
    let mut out = Raster::filled(w, total, Color::TRANSPARENT)?;
    crate::effects::paste(&mut out, r, 0, 0);
    let stride = w as usize * 4;
    let src = r.pixels();
    let px = out.pixels_mut();
    for i in 0..rh {
        // The row nearest the floor first.
        let from = (h - 1 - i) as usize;
        let to = (h + gap + i) as usize;
        let t = i as f32 / rh as f32;
        let fade = start * (1.0 - t) * (1.0 - t);
        let (Some(line), Some(dst)) = (src.get(from * stride..from * stride + stride), px.get_mut(to * stride..to * stride + stride)) else {
            continue;
        };
        for (d, s) in dst.as_chunks_mut::<4>().0.iter_mut().zip(line.as_chunks::<4>().0) {
            *d = [s[0], s[1], s[2], (f32::from(s[3]) * fade).round() as u8];
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> Raster {
        Raster::filled(80, 60, Color::RED).unwrap()
    }

    fn alpha(r: &Raster, x: u32, y: u32) -> u8 {
        r.pixel(x, y).map_or(0, |c| c.a)
    }

    #[test]
    fn wavy_and_faded_edges_keep_the_middle() {
        let r = red();
        let wavy = wavy_edge(&r, Edges::default(), 8.0, 20.0).unwrap();
        assert_eq!(wavy.pixel(40, 30), Some(Color::RED));
        assert_eq!(alpha(&wavy, 40, 1), 255, "the top doesn't wave");
        assert!((0..80).any(|x| alpha(&wavy, x, 57) < 128), "the bottom waves");
        assert!((0..80).any(|x| alpha(&wavy, x, 57) == 255), "in and out");
        let all = Edges { top: true, right: true, bottom: true, left: true };
        let faded = faded_edge(&r, all, 20.0);
        assert_eq!(faded.pixel(40, 30), Some(Color::RED));
        assert!(alpha(&faded, 0, 30) < 10 && alpha(&faded, 79, 30) < 10 && alpha(&faded, 40, 0) < 10);
        assert!(alpha(&faded, 10, 30) > 30 && alpha(&faded, 10, 30) < 230, "half way: {}", alpha(&faded, 10, 30));
        // Junk sizes are kept in range.
        assert!(wavy_edge(&r, all, f32::NAN, f32::INFINITY).is_ok());
        let _ = faded_edge(&Raster::single(Color::RED), all, 1e30);
    }

    #[test]
    fn a_bevel_lights_the_top_left_and_shades_the_bottom_right() {
        let grey = Raster::filled(40, 40, Color::rgb(128, 128, 128)).unwrap();
        let b = bevel(&grey, 6.0, 100.0);
        assert!(b.pixel(1, 20).unwrap().r > 180 && b.pixel(20, 1).unwrap().r > 180);
        assert!(b.pixel(38, 20).unwrap().r < 80 && b.pixel(20, 38).unwrap().r < 80);
        assert_eq!(b.pixel(20, 20), Some(Color::rgb(128, 128, 128)));
    }

    #[test]
    fn perspective_makes_the_far_side_smaller() {
        let r = red();
        let (out, map) = perspective(&r, 50.0, 0.0).unwrap();
        assert!(out.width() < 80 && out.height() == 60, "{}×{}", out.width(), out.height());
        // The near (left) side is all there, the far (right) side shorter.
        assert_eq!(alpha(&out, 1, 1), 255);
        assert_eq!(alpha(&out, out.width() - 2, 1), 0);
        assert_eq!(alpha(&out, out.width() - 2, 30), 255);
        // The map takes the old corners to the new ones.
        let tr = map.apply(Pt::new(80.0, 0.0));
        assert!((tr.x - out.width() as f32).abs() < 1.5 && tr.y > 5.0, "{tr:?}");
        assert_eq!(map.apply(Pt::new(0.0, 0.0)), Pt::new(0.0, 0.0));
        // Upwards and to the left, both; and none at all is the picture.
        let (both, _) = perspective(&r, -60.0, -40.0).unwrap();
        assert!(both.width() < 80 && both.height() < 60);
        let (flat, _) = perspective(&r, 0.0, 0.0).unwrap();
        assert_eq!((flat.width(), flat.height(), flat.pixel(40, 30)), (80, 60, Some(Color::RED)));
        assert!(perspective(&r, f32::NAN, 1e30).is_ok());
    }

    #[test]
    fn a_page_curl_takes_the_corner_away_and_folds_it_over() {
        let r = red();
        for corner in Corner::ALL {
            let out = page_curl(&r, corner, 30.0).unwrap();
            let (cx, cy) = match corner {
                Corner::TopLeft => (0, 0),
                Corner::TopRight => (79, 0),
                Corner::BottomRight => (79, 59),
                Corner::BottomLeft => (0, 59),
            };
            assert_eq!(alpha(&out, cx, cy), 0, "{corner:?}: the corner is gone");
            assert_eq!(out.pixel(40, 30), Some(Color::RED), "{corner:?}: the middle stays");
        }
        // The page's back is grey-white, inside the fold.
        let out = page_curl(&r, Corner::BottomRight, 30.0).unwrap();
        let back = out.pixel(66, 46).unwrap();
        assert!(back.g > 150 && back.b > 150, "{back:?}");
    }

    #[test]
    fn a_reflection_grows_the_picture_and_fades() {
        let r = red();
        let out = reflection(&r, 50.0, 60.0, 4.0).unwrap();
        assert_eq!((out.width(), out.height()), (80, 60 + 4 + 30));
        assert_eq!(out.pixel(10, 10), Some(Color::RED));
        assert_eq!(alpha(&out, 10, 62), 0, "the gap");
        let (near, far) = (alpha(&out, 10, 64), alpha(&out, 10, 93));
        assert!(near > 120 && near < 160 && far < 10, "{near} {far}");
    }
}
