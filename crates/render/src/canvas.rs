//! The drawing surface (a `tiny_skia::Pixmap`, premultiplied RGBA) and the pixel work done on
//! it directly: converting to and from [`Raster`], blurring, pixelating.

use screengrabcraft_model::Raster;
use tiny_skia::{IntSize, Pixmap};

/// A pixmap holding the raster's pixels (premultiplied).
pub fn to_pixmap(r: &Raster) -> Option<Pixmap> {
    let size = IntSize::from_wh(r.width(), r.height())?;
    let mut data = r.pixels().to_vec();
    for px in data.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        if a < 255 {
            for c in px.iter_mut().take(3) {
                *c = ((u16::from(*c) * a + 127) / 255) as u8;
            }
        }
    }
    Pixmap::from_vec(data, size)
}

/// The pixmap's pixels as a raster (straight alpha).
pub fn from_pixmap(p: &Pixmap) -> Option<Raster> {
    let mut data = p.data().to_vec();
    for px in data.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        if a > 0 && a < 255 {
            for c in px.iter_mut().take(3) {
                *c = ((u16::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Raster::new(p.width(), p.height(), data).ok()
}

/// A transparent pixmap, or none for an impossible size.
pub fn blank(w: u32, h: u32) -> Option<Pixmap> {
    if screengrabcraft_model::raster::checked_len(w, h).is_err() {
        return None;
    }
    Pixmap::new(w, h)
}

/// Box sizes whose three passes approximate a Gaussian of `sigma` (Kovesi's method).
fn boxes_for_gauss(sigma: f32) -> [usize; 3] {
    let n = 3.0f32;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wl = wl.max(1);
    let wu = wl + 2;
    let wlf = wl as f32;
    let m_ideal = (12.0 * sigma * sigma - n * wlf * wlf - 4.0 * n * wlf - 3.0 * n) / (-4.0 * wlf - 4.0);
    let m = m_ideal.round() as i64;
    let mut out = [0usize; 3];
    for (i, slot) in out.iter_mut().enumerate() {
        let w = if (i as i64) < m { wl } else { wu };
        *slot = usize::try_from((w - 1) / 2).unwrap_or(0);
    }
    out
}

/// One box pass of radius `r` along rows (`horizontal`) or columns, over the sub-rectangle
/// `(x0, y0, w, h)` of a `stride`-wide RGBA buffer, from `src` into `dst`.
fn box_pass(src: &[u8], dst: &mut [u8], stride: usize, area: (usize, usize, usize, usize), r: usize, horizontal: bool) {
    let (x0, y0, w, h) = area;
    let (lines, len) = if horizontal { (h, w) } else { (w, h) };
    if len == 0 {
        return;
    }
    let at = |line: usize, i: usize| -> usize { if horizontal { ((y0 + line) * stride + x0 + i) * 4 } else { ((y0 + i) * stride + x0 + line) * 4 } };
    let width = (2 * r + 1) as u32;
    for line in 0..lines {
        let mut sum = [0u32; 4];
        // The window starts centred on 0, with the edge pixel repeated beyond the ends.
        for k in 0..=2 * r {
            let i = k.saturating_sub(r).min(len - 1);
            let p = at(line, i);
            for (c, s) in sum.iter_mut().enumerate() {
                *s += u32::from(src.get(p + c).copied().unwrap_or(0));
            }
        }
        for i in 0..len {
            let p = at(line, i);
            for (c, s) in sum.iter().enumerate() {
                if let Some(d) = dst.get_mut(p + c) {
                    *d = ((*s + width / 2) / width) as u8;
                }
            }
            let out_i = i.saturating_sub(r);
            let in_i = (i + r + 1).min(len - 1);
            let (po, pi) = (at(line, out_i), at(line, in_i));
            for (c, s) in sum.iter_mut().enumerate() {
                *s = (*s + u32::from(src.get(pi + c).copied().unwrap_or(0))).saturating_sub(u32::from(src.get(po + c).copied().unwrap_or(0)));
            }
        }
    }
}

/// Gaussian-like blur of radius `sigma` over part of a pixmap (premultiplied, so colours don't
/// darken at transparent edges). Pixels outside the area are not read: edges repeat.
pub fn blur_area(p: &mut Pixmap, x: u32, y: u32, w: u32, h: u32, sigma: f32) {
    if !sigma.is_finite() || sigma < 0.5 || w == 0 || h == 0 {
        return;
    }
    let (pw, ph) = (p.width(), p.height());
    let (x, y) = (x.min(pw), y.min(ph));
    let (w, h) = (w.min(pw - x) as usize, h.min(ph - y) as usize);
    if w == 0 || h == 0 {
        return;
    }
    let stride = pw as usize;
    let area = (x as usize, y as usize, w, h);
    let data = p.data_mut();
    let mut tmp = data.to_vec();
    for r in boxes_for_gauss(sigma.min(250.0)) {
        box_pass(data, &mut tmp, stride, area, r, true);
        box_pass(&tmp, data, stride, area, r, false);
    }
}

/// Blurs the whole pixmap.
pub fn blur(p: &mut Pixmap, sigma: f32) {
    let (w, h) = (p.width(), p.height());
    blur_area(p, 0, 0, w, h, sigma);
}

/// Replaces part of a pixmap with squares of `size` pixels, each the average of what it covers.
pub fn pixelate_area(p: &mut Pixmap, x: u32, y: u32, w: u32, h: u32, size: u32) {
    let size = size.clamp(2, 512);
    let (pw, ph) = (p.width(), p.height());
    let (x1, y1) = (x.saturating_add(w).min(pw), y.saturating_add(h).min(ph));
    let stride = pw as usize;
    let data = p.data_mut();
    let mut by = y;
    while by < y1 {
        let bh = size.min(y1 - by);
        let mut bx = x;
        while bx < x1 {
            let bw = size.min(x1 - bx);
            let mut sum = [0u64; 4];
            for yy in by..by + bh {
                for xx in bx..bx + bw {
                    let i = (yy as usize * stride + xx as usize) * 4;
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += u64::from(data.get(i + c).copied().unwrap_or(0));
                    }
                }
            }
            let n = u64::from(bw) * u64::from(bh);
            let avg = sum.map(|s| (s / n.max(1)) as u8);
            for yy in by..by + bh {
                for xx in bx..bx + bw {
                    let i = (yy as usize * stride + xx as usize) * 4;
                    if let Some(px) = data.get_mut(i..i + 4) {
                        px.copy_from_slice(&avg);
                    }
                }
            }
            bx += bw;
        }
        by += bh;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Color;

    #[test]
    fn round_trips_premultiplied() {
        let mut r = Raster::filled(2, 1, Color::rgba(200, 100, 50, 128)).unwrap();
        r.pixels_mut()[4..8].copy_from_slice(&[10, 20, 30, 255]);
        let p = to_pixmap(&r).unwrap();
        assert_eq!(&p.data()[..4], &[100, 50, 25, 128]);
        let back = from_pixmap(&p).unwrap();
        let px = back.pixel(0, 0).unwrap();
        assert!((i32::from(px.r) - 200).abs() <= 2 && (i32::from(px.g) - 100).abs() <= 2);
        assert_eq!(back.pixel(1, 0), Some(Color::rgb(10, 20, 30)));
    }

    #[test]
    fn blur_spreads_and_keeps_totals() {
        let mut p = Pixmap::new(21, 1).unwrap();
        p.data_mut()[10 * 4..10 * 4 + 4].copy_from_slice(&[255, 255, 255, 255]);
        blur(&mut p, 3.0);
        let centre = p.data()[10 * 4];
        let side = p.data()[7 * 4];
        assert!(centre < 255 && centre > 0 && side > 0 && side < centre, "{centre} {side}");
        // A flat colour stays flat.
        let mut flat = Pixmap::new(8, 8).unwrap();
        flat.fill(tiny_skia::Color::from_rgba8(40, 80, 120, 255));
        blur(&mut flat, 5.0);
        assert!(flat.data().chunks(4).all(|c| c == [40, 80, 120, 255]));
        // Nothing to do for silly radii or areas.
        blur_area(&mut flat, 100, 100, 5, 5, 3.0);
        blur(&mut flat, f32::NAN);
    }

    #[test]
    fn pixelate_averages_blocks() {
        let mut p = Pixmap::new(4, 2).unwrap();
        for (i, c) in p.data_mut().chunks_mut(4).enumerate() {
            let v = if i % 2 == 0 { 0 } else { 200 };
            c.copy_from_slice(&[v, v, v, 255]);
        }
        pixelate_area(&mut p, 0, 0, 4, 2, 2);
        assert!(p.data().chunks(4).all(|c| c == [100, 100, 100, 255]));
        pixelate_area(&mut p, 3, 1, 100, 100, 8);
    }

    #[test]
    fn boxes() {
        for sigma in [0.5f32, 1.0, 3.0, 10.0, 50.0] {
            let b = boxes_for_gauss(sigma);
            assert!(b.iter().all(|r| *r < 200), "{sigma}: {b:?}");
        }
    }
}
