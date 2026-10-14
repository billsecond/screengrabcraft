//! Changes to the whole picture: crop, resize, turn, flip, more canvas, trim, and the effects
//! (border, drop shadow, torn edge, round corners, greyscale, invert, brightness and contrast).
//!
//! Each returns a new raster; the engine moves the annotations to match.

use image::RgbaImage;
use screengrabcraft_model::{Color, Raster, RasterError, Rect};

use crate::canvas;

fn image_of(r: &Raster) -> Option<RgbaImage> {
    RgbaImage::from_raw(r.width(), r.height(), r.pixels().to_vec())
}

fn raster_of(img: RgbaImage) -> Result<Raster, RasterError> {
    let (w, h) = img.dimensions();
    Raster::new(w, h, img.into_raw())
}

fn bad(w: u32, h: u32) -> RasterError {
    RasterError::Empty(w, h)
}

/// The part inside the rectangle (clipped to the picture).
pub fn crop(r: &Raster, area: Rect) -> Result<Raster, RasterError> {
    let (x, y, w, h) = area.pixel_bounds(r.width(), r.height()).ok_or(bad(0, 0))?;
    r.crop(x, y, w, h).ok_or(bad(w, h))
}

/// The picture scaled to `w`×`h` (Lanczos when shrinking, Catmull-Rom when enlarging).
pub fn resize(r: &Raster, w: u32, h: u32) -> Result<Raster, RasterError> {
    screengrabcraft_model::raster::checked_len(w, h)?;
    if (w, h) == (r.width(), r.height()) {
        return Ok(r.clone());
    }
    let img = image_of(r).ok_or(bad(r.width(), r.height()))?;
    let filter = if w < r.width() || h < r.height() { image::imageops::FilterType::Lanczos3 } else { image::imageops::FilterType::CatmullRom };
    raster_of(image::imageops::resize(&img, w, h, filter))
}

/// Turned clockwise by `quarters` quarter turns.
pub fn rotate(r: &Raster, quarters: u32) -> Result<Raster, RasterError> {
    let img = image_of(r).ok_or(bad(r.width(), r.height()))?;
    raster_of(match quarters % 4 {
        1 => image::imageops::rotate90(&img),
        2 => image::imageops::rotate180(&img),
        3 => image::imageops::rotate270(&img),
        _ => img,
    })
}

/// Mirrored left to right (`horizontal`) or top to bottom.
pub fn flip(r: &Raster, horizontal: bool) -> Result<Raster, RasterError> {
    let img = image_of(r).ok_or(bad(r.width(), r.height()))?;
    raster_of(if horizontal { image::imageops::flip_horizontal(&img) } else { image::imageops::flip_vertical(&img) })
}

/// More canvas on each side (negative takes it away), the new part in `fill`.
pub fn canvas_size(r: &Raster, left: i32, top: i32, right: i32, bottom: i32, fill: Color) -> Result<Raster, RasterError> {
    let w = i64::from(r.width()) + i64::from(left) + i64::from(right);
    let h = i64::from(r.height()) + i64::from(top) + i64::from(bottom);
    let (w, h) = (u32::try_from(w).map_err(|_| bad(0, 0))?, u32::try_from(h).map_err(|_| bad(0, 0))?);
    let mut out = Raster::filled(w, h, fill)?;
    paste(&mut out, r, i64::from(left), i64::from(top));
    Ok(out)
}

/// Copies `src` onto `dst` with its top left at `(x, y)` (replacing, not blending), clipped.
pub fn paste(dst: &mut Raster, src: &Raster, x: i64, y: i64) {
    let (dw, dh) = (i64::from(dst.width()), i64::from(dst.height()));
    let (sw, sh) = (i64::from(src.width()), i64::from(src.height()));
    let x0 = x.max(0);
    let x1 = (x + sw).min(dw);
    if x1 <= x0 {
        return;
    }
    let n = usize::try_from((x1 - x0) * 4).unwrap_or(0);
    let src_px = src.pixels();
    let dst_stride = dst.width() as usize * 4;
    let src_stride = src.width() as usize * 4;
    let out = dst.pixels_mut();
    for sy in 0..sh {
        let dy = y + sy;
        if dy < 0 || dy >= dh {
            continue;
        }
        let (Ok(dy), Ok(sy)) = (usize::try_from(dy), usize::try_from(sy)) else { continue };
        let (Ok(dx), Ok(sx)) = (usize::try_from(x0), usize::try_from(x0 - x)) else { continue };
        let d = dy * dst_stride + dx * 4;
        let s = sy * src_stride + sx * 4;
        if let (Some(dst_row), Some(src_row)) = (out.get_mut(d..d + n), src_px.get(s..s + n)) {
            dst_row.copy_from_slice(src_row);
        }
    }
}

/// The rectangle left when borders of one colour (the top left pixel's, within `tolerance`
/// per channel) are trimmed away; the whole picture when there are none.
pub fn trim_bounds(r: &Raster, tolerance: u8) -> Rect {
    let (w, h) = (r.width(), r.height());
    let Some(corner) = r.pixel(0, 0) else { return Rect::new(0.0, 0.0, w as f32, h as f32) };
    let near = |c: Color| {
        let d = |a: u8, b: u8| a.abs_diff(b) <= tolerance;
        d(c.r, corner.r) && d(c.g, corner.g) && d(c.b, corner.b) && d(c.a, corner.a)
    };
    let row_is_border = |y: u32| (0..w).all(|x| r.pixel(x, y).is_some_and(near));
    let col_is_border = |x: u32, y0: u32, y1: u32| (y0..y1).all(|y| r.pixel(x, y).is_some_and(near));
    let mut top = 0;
    while top < h && row_is_border(top) {
        top += 1;
    }
    if top == h {
        return Rect::new(0.0, 0.0, w as f32, h as f32);
    }
    let mut bottom = h;
    while bottom > top && row_is_border(bottom - 1) {
        bottom -= 1;
    }
    let mut left = 0;
    while left < w && col_is_border(left, top, bottom) {
        left += 1;
    }
    let mut right = w;
    while right > left && col_is_border(right - 1, top, bottom) {
        right -= 1;
    }
    Rect::new(left as f32, top as f32, (right - left) as f32, (bottom - top) as f32)
}

/// A border of `width` pixels in `color` round the picture.
pub fn border(r: &Raster, width: u32, color: Color) -> Result<Raster, RasterError> {
    let w = i32::try_from(width.min(2000)).unwrap_or(0);
    canvas_size(r, w, w, w, w, color)
}

/// The margins a drop shadow adds: left, top, right, bottom.
pub fn shadow_margins(offset: u32, softness: u32) -> (u32, u32, u32, u32) {
    let s = softness.min(200) * 2 + 2;
    let o = offset.min(200);
    (s, s, s + o, s + o)
}

/// The picture on a transparent canvas with a soft shadow falling `offset` pixels down and to
/// the right.
pub fn drop_shadow(r: &Raster, offset: u32, softness: u32, color: Color) -> Result<Raster, RasterError> {
    let (l, t, rr, b) = shadow_margins(offset, softness);
    let mut shadow = canvas_size(r, l as i32, t as i32, rr as i32, b as i32, Color::TRANSPARENT)?;
    {
        let (sw, src) = (shadow.width() as usize, r.pixels());
        let o = offset.min(200) as usize;
        let px = shadow.pixels_mut();
        px.fill(0);
        for y in 0..r.height() as usize {
            for x in 0..r.width() as usize {
                let a = src.get((y * r.width() as usize + x) * 4 + 3).copied().unwrap_or(0);
                let i = ((y + t as usize + o) * sw + x + l as usize + o) * 4;
                if let Some(p) = px.get_mut(i..i + 4) {
                    let a = (u16::from(a) * u16::from(color.a) / 255) as u8;
                    p.copy_from_slice(&[color.r, color.g, color.b, a]);
                }
            }
        }
    }
    let mut pix = canvas::to_pixmap(&shadow).ok_or(bad(shadow.width(), shadow.height()))?;
    canvas::blur(&mut pix, softness.max(1) as f32);
    let picture = canvas::to_pixmap(r).ok_or(bad(r.width(), r.height()))?;
    pix.draw_pixmap(l as i32, t as i32, picture.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
    canvas::from_pixmap(&pix).ok_or(bad(0, 0))
}

/// Which edges a torn-edge effect tears.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Edges {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl Default for Edges {
    fn default() -> Edges {
        Edges { top: false, right: false, bottom: true, left: false }
    }
}

/// A small, fixed pseudo-random sequence, so a torn edge looks the same each time.
struct Jitter(u64);

impl Jitter {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / ((1u64 << 31) as f32)
    }
}

/// The picture with ragged, torn-paper edges on the chosen sides, `depth` pixels deep.
pub fn torn_edge(r: &Raster, edges: Edges, depth: u32) -> Result<Raster, RasterError> {
    let (w, h) = (r.width() as f32, r.height() as f32);
    let d = (depth.clamp(4, 200) as f32).min(w / 3.0).min(h / 3.0).max(1.0);
    let mut rng = Jitter(0x5EED_CAFE);
    let mut pb = tiny_skia::PathBuilder::new();
    let tooth = (d * 0.9).max(4.0);
    let mut edge = |pb: &mut tiny_skia::PathBuilder, a: (f32, f32), b: (f32, f32), inward: (f32, f32), torn: bool| {
        if !torn {
            pb.line_to(b.0, b.1);
            return;
        }
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let n = (len / tooth).ceil().max(1.0) as usize;
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let depth = if i == n { 0.0 } else { d * (0.2 + 0.8 * rng.next()) };
            pb.line_to(a.0 + (b.0 - a.0) * t + inward.0 * depth, a.1 + (b.1 - a.1) * t + inward.1 * depth);
        }
    };
    pb.move_to(0.0, 0.0);
    edge(&mut pb, (0.0, 0.0), (w, 0.0), (0.0, 1.0), edges.top);
    edge(&mut pb, (w, 0.0), (w, h), (-1.0, 0.0), edges.right);
    edge(&mut pb, (w, h), (0.0, h), (0.0, -1.0), edges.bottom);
    edge(&mut pb, (0.0, h), (0.0, 0.0), (1.0, 0.0), edges.left);
    pb.close();
    let path = pb.finish().ok_or(bad(r.width(), r.height()))?;
    let mut mask = tiny_skia::Mask::new(r.width(), r.height()).ok_or(bad(r.width(), r.height()))?;
    mask.fill_path(&path, tiny_skia::FillRule::Winding, true, tiny_skia::Transform::identity());
    let mut out = r.clone();
    let m = mask.data();
    for (px, a) in out.pixels_mut().as_chunks_mut::<4>().0.iter_mut().zip(m.iter()) {
        px[3] = (u16::from(px[3]) * u16::from(*a) / 255) as u8;
    }
    Ok(out)
}

/// The picture with its corners rounded to `radius` (made transparent outside).
pub fn round_corners(r: &Raster, radius: f32) -> Result<Raster, RasterError> {
    let rect = Rect::new(0.0, 0.0, r.width() as f32, r.height() as f32);
    let path = crate::draw::rounded_rect(rect, radius).ok_or(bad(r.width(), r.height()))?;
    let mut mask = tiny_skia::Mask::new(r.width(), r.height()).ok_or(bad(r.width(), r.height()))?;
    mask.fill_path(&path, tiny_skia::FillRule::Winding, true, tiny_skia::Transform::identity());
    let mut out = r.clone();
    for (px, a) in out.pixels_mut().as_chunks_mut::<4>().0.iter_mut().zip(mask.data().iter()) {
        px[3] = (u16::from(px[3]) * u16::from(*a) / 255) as u8;
    }
    Ok(out)
}

/// Greys (by luminance).
pub fn grayscale(r: &Raster) -> Raster {
    let mut out = r.clone();
    for px in out.pixels_mut().as_chunks_mut::<4>().0 {
        let y = (u32::from(px[0]) * 2126 + u32::from(px[1]) * 7152 + u32::from(px[2]) * 722) / 10_000;
        let y = y.min(255) as u8;
        px[0] = y;
        px[1] = y;
        px[2] = y;
    }
    out
}

/// Colours inverted (alpha kept).
pub fn invert(r: &Raster) -> Raster {
    let mut out = r.clone();
    for px in out.pixels_mut().as_chunks_mut::<4>().0 {
        px[0] = 255 - px[0];
        px[1] = 255 - px[1];
        px[2] = 255 - px[2];
    }
    out
}

/// Brightness (-100 to 100) and contrast (-100 to 100).
pub fn brightness_contrast(r: &Raster, brightness: f32, contrast: f32) -> Raster {
    let b = if brightness.is_finite() { brightness.clamp(-100.0, 100.0) * 2.55 } else { 0.0 };
    let c = if contrast.is_finite() { contrast.clamp(-100.0, 100.0) } else { 0.0 };
    let f = (259.0 * (c * 2.55 + 255.0)) / (255.0 * (259.0 - c * 2.55));
    let lut: Vec<u8> = (0..=255u32).map(|v| (f * (v as f32 - 128.0) + 128.0 + b).round().clamp(0.0, 255.0) as u8).collect();
    let mut out = r.clone();
    for px in out.pixels_mut().as_chunks_mut::<4>().0 {
        for c in px.iter_mut().take(3) {
            *c = lut.get(usize::from(*c)).copied().unwrap_or(*c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pic() -> Raster {
        let mut r = Raster::filled(4, 2, Color::WHITE).unwrap();
        r.pixels_mut()[..4].copy_from_slice(&[255, 0, 0, 255]);
        r
    }

    #[test]
    fn turns_and_flips() {
        let r = pic();
        let q = rotate(&r, 1).unwrap();
        assert_eq!((q.width(), q.height()), (2, 4));
        assert_eq!(q.pixel(1, 0), Some(Color::rgb(255, 0, 0)), "top left goes top right");
        assert_eq!(rotate(&r, 4).unwrap(), r);
        assert_eq!(flip(&r, true).unwrap().pixel(3, 0), Some(Color::rgb(255, 0, 0)));
        assert_eq!(flip(&r, false).unwrap().pixel(0, 1), Some(Color::rgb(255, 0, 0)));
    }

    #[test]
    fn canvas_and_trim() {
        let r = pic();
        let big = canvas_size(&r, 2, 3, 1, 0, Color::BLACK).unwrap();
        assert_eq!((big.width(), big.height()), (7, 5));
        assert_eq!(big.pixel(2, 3), Some(Color::rgb(255, 0, 0)));
        assert_eq!(big.pixel(0, 0), Some(Color::BLACK));
        assert_eq!(trim_bounds(&big, 0), Rect::new(2.0, 3.0, 4.0, 2.0));
        let small = canvas_size(&r, -1, 0, -1, 0, Color::BLACK).unwrap();
        assert_eq!((small.width(), small.height()), (2, 2));
        assert!(canvas_size(&r, -10, 0, 0, 0, Color::BLACK).is_err());
        let flat = Raster::filled(3, 3, Color::WHITE).unwrap();
        assert_eq!(trim_bounds(&flat, 0), Rect::new(0.0, 0.0, 3.0, 3.0));
    }

    #[test]
    fn resize_and_crop() {
        let r = Raster::filled(100, 50, Color::RED).unwrap();
        let s = resize(&r, 10, 5).unwrap();
        assert_eq!((s.width(), s.height()), (10, 5));
        assert!(resize(&r, 0, 5).is_err());
        let c = crop(&r, Rect::new(90.0, 40.0, 50.0, 50.0)).unwrap();
        assert_eq!((c.width(), c.height()), (10, 10));
        assert!(crop(&r, Rect::new(200.0, 0.0, 5.0, 5.0)).is_err());
    }

    #[test]
    fn effects_keep_or_grow_sizes() {
        let r = Raster::filled(60, 40, Color::RED).unwrap();
        let b = border(&r, 5, Color::BLACK).unwrap();
        assert_eq!((b.width(), b.height()), (70, 50));
        let s = drop_shadow(&r, 6, 8, Color::rgba(0, 0, 0, 120)).unwrap();
        let (l, t, rr, bb) = shadow_margins(6, 8);
        assert_eq!((s.width(), s.height()), (60 + l + rr, 40 + t + bb));
        assert_eq!(s.pixel(l + 10, t + 10), Some(Color::RED));
        assert!(s.pixel(l + 62, t + 42).is_some_and(|c| c.a > 0), "the shadow is there");
        assert_eq!(s.pixel(0, 0).map(|c| c.a), Some(0));
        let torn = torn_edge(&r, Edges::default(), 10).unwrap();
        assert_eq!(torn.pixel(30, 5), Some(Color::RED), "untorn top");
        assert!((0..60).any(|x| torn.pixel(x, 39).is_some_and(|c| c.a < 255)), "torn bottom");
        let round = round_corners(&r, 10.0).unwrap();
        assert_eq!(round.pixel(0, 0).map(|c| c.a), Some(0));
        assert_eq!(round.pixel(30, 20), Some(Color::RED));
        assert_eq!(grayscale(&r).pixel(0, 0).map(|c| c.r == c.g && c.g == c.b), Some(true));
        assert_eq!(invert(&r).pixel(0, 0), Some(Color::rgb(0x1A, 0xC6, 0xCA)));
        let bright = brightness_contrast(&Raster::filled(1, 1, Color::rgb(100, 100, 100)).unwrap(), 50.0, 0.0);
        assert!(bright.pixel(0, 0).is_some_and(|c| c.r > 200));
    }
}
