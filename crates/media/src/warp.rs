//! A picture pinned onto a slanted surface: its four corners put anywhere, the picture drawn in
//! perspective between them (a screen seen at an angle, a poster on a wall). Each pixel inside the
//! four corners is looked up in the picture by the plane's own projection (a homography), and
//! blended in.

use tiny_skia::Pixmap;

/// The corners, in order: top left, top right, bottom right, bottom left (pixels of the picture
/// drawn on).
pub type Quad = [[f32; 2]; 4];

/// The projection taking the square 0..1 × 0..1 onto `q` (its corners in the order above), as a
/// 3 × 3 matrix by rows: none if the corners make no surface (three in a line, or not numbers).
pub fn square_to(q: &Quad) -> Option<[f64; 9]> {
    if q.iter().flatten().any(|v| !v.is_finite()) {
        return None;
    }
    let [[x0, y0], [x1, y1], [x2, y2], [x3, y3]] = q.map(|[x, y]| [f64::from(x), f64::from(y)]);
    let (sx, sy) = (x0 - x1 + x2 - x3, y0 - y1 + y2 - y3);
    let (dx1, dy1, dx2, dy2) = (x1 - x2, y1 - y2, x3 - x2, y3 - y2);
    let den = dx1 * dy2 - dx2 * dy1;
    if den.abs() < 1e-9 {
        return None;
    }
    let g = (sx * dy2 - dx2 * sy) / den;
    let h = (dx1 * sy - sx * dy1) / den;
    Some([x1 - x0 + g * x1, x3 - x0 + h * x3, x0, y1 - y0 + g * y1, y3 - y0 + h * y3, y0, g, h, 1.0])
}

/// The matrix undoing `m`: none if there's none.
pub fn invert(m: &[f64; 9]) -> Option<[f64; 9]> {
    let [a, b, c, d, e, f, g, h, i] = *m;
    let (ca, cb, cc) = (e * i - f * h, -(d * i - f * g), d * h - e * g);
    let det = a * ca + b * cb + c * cc;
    if det.abs() < 1e-12 || !det.is_finite() {
        return None;
    }
    let k = 1.0 / det;
    Some([
        ca * k,
        -(b * i - c * h) * k,
        (b * f - c * e) * k,
        cb * k,
        (a * i - c * g) * k,
        -(a * f - c * d) * k,
        cc * k,
        -(a * h - b * g) * k,
        (a * e - b * d) * k,
    ])
}

/// Where `m` takes the point (x, y): none where it goes off to infinity.
pub fn apply(m: &[f64; 9], x: f64, y: f64) -> Option<(f64, f64)> {
    let w = m[6] * x + m[7] * y + m[8];
    if w.abs() < 1e-9 {
        return None;
    }
    Some(((m[0] * x + m[1] * y + m[2]) / w, (m[3] * x + m[4] * y + m[5]) / w))
}

/// The pixel of `pic` at (u, v) (shares of it), blended from the four nearest (premultiplied).
fn sample(pic: &Pixmap, u: f64, v: f64) -> [f32; 4] {
    let (w, h) = (pic.width() as f64, pic.height() as f64);
    let (x, y) = (u * w - 0.5, v * h - 0.5);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
    let data = pic.data();
    let px = |xi: f64, yi: f64| -> [f32; 4] {
        let (xi, yi) = (xi.clamp(0.0, w - 1.0) as usize, yi.clamp(0.0, h - 1.0) as usize);
        let k = (yi * pic.width() as usize + xi) * 4;
        match data.get(k..k + 4) {
            Some(p) => [f32::from(p[0]), f32::from(p[1]), f32::from(p[2]), f32::from(p[3])],
            None => [0.0; 4],
        }
    };
    let (a, b, c, d) = (px(x0, y0), px(x0 + 1.0, y0), px(x0, y0 + 1.0), px(x0 + 1.0, y0 + 1.0));
    let mut out = [0.0f32; 4];
    for (k, o) in out.iter_mut().enumerate() {
        let top = a[k] + (b[k] - a[k]) * fx;
        let bottom = c[k] + (d[k] - c[k]) * fx;
        *o = top + (bottom - top) * fy;
    }
    out
}

/// Draws `pic` onto `out` with its corners at `q` (in perspective between them), `alpha` as
/// see-through (0 to 1), what `crop` cuts off its sides left out (shares of it: left, top, right,
/// bottom). Nothing is drawn if the corners make no surface.
pub fn draw(out: &mut Pixmap, pic: &Pixmap, crop: [f32; 4], q: &Quad, alpha: f32) {
    let c = crop.map(|v| if v.is_finite() { f64::from(v.clamp(0.0, 0.45)) } else { 0.0 });
    let (cw, ch) = (1.0 - c[0] - c[2], 1.0 - c[1] - c[3]);
    let alpha = if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 0.0 };
    if alpha <= 0.0 || pic.width() == 0 || pic.height() == 0 {
        return;
    }
    let Some(back) = square_to(q).and_then(|m| invert(&m)) else { return };
    let (ow, oh) = (out.width() as f32, out.height() as f32);
    // Only where the corners reach, on the picture.
    let lo = |k: usize| q.iter().map(|c| c[k]).fold(f32::INFINITY, f32::min);
    let hi = |k: usize| q.iter().map(|c| c[k]).fold(f32::NEG_INFINITY, f32::max);
    let (x0, x1) = (lo(0).floor().clamp(0.0, ow) as usize, hi(0).ceil().clamp(0.0, ow) as usize);
    let (y0, y1) = (lo(1).floor().clamp(0.0, oh) as usize, hi(1).ceil().clamp(0.0, oh) as usize);
    let width = out.width() as usize;
    let data = out.data_mut();
    for y in y0..y1 {
        for x in x0..x1 {
            let Some((u, v)) = apply(&back, x as f64 + 0.5, y as f64 + 0.5) else { continue };
            if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                continue;
            }
            let s = sample(pic, c[0] + u * cw, c[1] + v * ch);
            let a = s[3] / 255.0 * alpha;
            if a <= 0.0 {
                continue;
            }
            let k = (y * width + x) * 4;
            if let Some(d) = data.get_mut(k..k + 4) {
                // Premultiplied: the picture's over what's there.
                for c in 0..4 {
                    let src = s[c] * alpha;
                    d[c] = (src + f32::from(d[c]) * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_square_goes_onto_its_corners_and_back() {
        let q: Quad = [[10.0, 20.0], [110.0, 30.0], [100.0, 120.0], [15.0, 100.0]];
        let m = square_to(&q).unwrap();
        for ([u, v], c) in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]].into_iter().zip(q) {
            let (x, y) = apply(&m, u, v).unwrap();
            assert!((x - f64::from(c[0])).abs() < 1e-6 && (y - f64::from(c[1])).abs() < 1e-6, "{x} {y} {c:?}");
        }
        let back = invert(&m).unwrap();
        let (u, v) = apply(&back, 10.0, 20.0).unwrap();
        assert!(u.abs() < 1e-6 && v.abs() < 1e-6);
        // Corners in a line, or not numbers: no surface.
        assert!(square_to(&[[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0]]).is_none());
        assert!(square_to(&[[f32::NAN, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).is_none());
    }

    #[test]
    fn a_picture_is_drawn_between_its_corners_and_nowhere_else() {
        let mut red = Pixmap::new(20, 20).unwrap();
        red.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let mut out = Pixmap::new(100, 100).unwrap();
        // A slanted shape, narrower at the top.
        let q: Quad = [[40.0, 10.0], [60.0, 10.0], [90.0, 90.0], [10.0, 90.0]];
        draw(&mut out, &red, [0.0; 4], &q, 1.0);
        let at = |x: usize, y: usize| out.data()[(y * 100 + x) * 4];
        assert_eq!(at(50, 50), 255, "inside");
        assert_eq!(at(15, 15), 0, "outside the narrow top");
        assert_eq!(at(15, 85), 255, "inside the wide bottom");
        // Half see-through; junk corners draw nothing and never panic.
        let mut faint = Pixmap::new(100, 100).unwrap();
        draw(&mut faint, &red, [0.0; 4], &q, 0.5);
        assert!((120..=135).contains(&faint.data()[(50 * 100 + 50) * 4]));
        draw(&mut faint, &red, [f32::NAN, 0.9, -1.0, 0.0], &[[f32::INFINITY, 0.0], [1e30, -1e30], [0.0, 0.0], [0.0, 0.0]], 1.0);
        draw(&mut faint, &red, [0.0; 4], &[[-1e9, -1e9], [1e9, -1e9], [1e9, 1e9], [-1e9, 1e9]], f32::NAN);
    }
}
