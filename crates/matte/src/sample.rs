//! The picture as a model wants it (its square, RGB planes, normalised), and the model's answer
//! as a matte.
//!
//! A picture of any size is first averaged down in blocks to at most twice the model's side
//! (one pass over the pixels, little memory, however long a scrolling capture is), then filtered
//! to the square with a triangle filter as wide as the step, so nothing aliases.

use screengrabcraft_model::Raster;
use screengrabcraft_render::matte::{Matte, MatteError};

/// The picture's RGB as floats (0 to 255, three to a pixel), averaged over blocks so neither side
/// is longer than `most`: its width, height and values.
fn shrink(r: &Raster, most: usize) -> (usize, usize, Vec<f32>) {
    let (w, h) = (r.width() as usize, r.height() as usize);
    let most = most.max(1);
    let (kx, ky) = (w.div_ceil(most).max(1), h.div_ceil(most).max(1));
    let (ow, oh) = (w.div_ceil(kx), h.div_ceil(ky));
    let mut sums = vec![0f32; ow * oh * 3];
    let mut counts = vec![0u32; ow * oh];
    let pixels = r.pixels().as_chunks::<4>().0;
    for (y, row) in pixels.chunks(w.max(1)).enumerate() {
        let oy = y / ky;
        for (x, px) in row.iter().enumerate() {
            let o = oy * ow + x / kx;
            if let Some([r, g, b]) = sums.get_mut(o * 3..o * 3 + 3) {
                *r += f32::from(px[0]);
                *g += f32::from(px[1]);
                *b += f32::from(px[2]);
            }
            if let Some(c) = counts.get_mut(o) {
                *c += 1;
            }
        }
    }
    for (s, c) in sums.as_chunks_mut::<3>().0.iter_mut().zip(&counts) {
        if *c > 1 {
            for v in s.iter_mut() {
                *v /= *c as f32;
            }
        }
    }
    (ow, oh, sums)
}

/// For each of `dst` places along an axis of `src`, the first source place it takes from and
/// the weights of it and those after it (a triangle as wide as the step, at least one place).
fn taps(src: usize, dst: usize) -> Vec<(usize, Vec<f32>)> {
    let scale = src as f32 / dst.max(1) as f32;
    let support = scale.max(1.0);
    (0..dst)
        .map(|i| {
            let centre = (i as f32 + 0.5) * scale;
            let lo = ((centre - support).floor().max(0.0) as usize).min(src.saturating_sub(1));
            let hi = ((centre + support).ceil().max(0.0) as usize).clamp(lo + 1, src.max(lo + 1));
            let mut weights: Vec<f32> = (lo..hi).map(|j| (1.0 - ((j as f32 + 0.5 - centre) / support).abs()).max(0.0)).collect();
            let sum: f32 = weights.iter().sum();
            if sum > 0.0 {
                for w in &mut weights {
                    *w /= sum;
                }
            } else {
                weights = vec![1.0];
            }
            (lo, weights)
        })
        .collect()
}

/// `src` (`w`×`h`, three values a pixel) filtered to `side`×`side`.
fn resample(src: &[f32], w: usize, h: usize, side: usize) -> Vec<f32> {
    let (xt, yt) = (taps(w, side), taps(h, side));
    let mut across = vec![0f32; h * side * 3];
    for y in 0..h {
        for (ox, (lo, weights)) in xt.iter().enumerate() {
            let mut acc = [0f32; 3];
            for (k, wt) in weights.iter().enumerate() {
                let i = (y * w + lo + k) * 3;
                if let Some([r, g, b]) = src.get(i..i + 3) {
                    acc[0] += r * wt;
                    acc[1] += g * wt;
                    acc[2] += b * wt;
                }
            }
            let o = (y * side + ox) * 3;
            if let Some(d) = across.get_mut(o..o + 3) {
                d.copy_from_slice(&acc);
            }
        }
    }
    let mut out = vec![0f32; side * side * 3];
    for (oy, (lo, weights)) in yt.iter().enumerate() {
        for x in 0..side {
            let mut acc = [0f32; 3];
            for (k, wt) in weights.iter().enumerate() {
                let i = ((lo + k) * side + x) * 3;
                if let Some([r, g, b]) = across.get(i..i + 3) {
                    acc[0] += r * wt;
                    acc[1] += g * wt;
                    acc[2] += b * wt;
                }
            }
            let o = (oy * side + x) * 3;
            if let Some(d) = out.get_mut(o..o + 3) {
                d.copy_from_slice(&acc);
            }
        }
    }
    out
}

/// The model's input: the picture at `side`×`side`, red, green and blue planes one after the
/// other, scaled by its brightest value and normalised by `mean` and `spread` (as rembg does).
pub(crate) fn input(picture: &Raster, side: usize, (mean, spread): ([f32; 3], [f32; 3])) -> Result<Vec<f32>, MatteError> {
    if side == 0 {
        return Err(MatteError::Model("the model's picture has no size".to_string()));
    }
    let (w, h, small) = shrink(picture, side * 2);
    let square = resample(&small, w, h, side);
    let brightest = square.iter().copied().fold(1e-6f32, f32::max);
    let n = side * side;
    let mut out = vec![0f32; 3 * n];
    for (c, plane) in out.chunks_exact_mut(n).enumerate() {
        let (m, s) = (mean.get(c).copied().unwrap_or(0.0), spread.get(c).copied().unwrap_or(1.0).max(1e-6));
        for (v, px) in plane.iter_mut().zip(square.as_chunks::<3>().0) {
            *v = (px.get(c).copied().unwrap_or(0.0) / brightest - m) / s;
        }
    }
    Ok(out)
}

/// The model's answer (`side`×`side`, higher on the subject) as a matte: stretched so its
/// lowest is 0 and its highest 255, as rembg does. A flat answer tells nothing apart, and keeps
/// everything.
pub(crate) fn to_matte(values: &[f32], side: usize) -> Result<Matte, MatteError> {
    let (lo, hi) = values.iter().filter(|v| v.is_finite()).fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    let range = hi - lo;
    let alpha: Vec<u8> = if range > 1e-4 {
        values.iter().map(|v| if v.is_finite() { ((v - lo) / range * 255.0).round().clamp(0.0, 255.0) as u8 } else { 255 }).collect()
    } else {
        vec![255; values.len()]
    };
    Ok(Matte::new(side as u32, side as u32, alpha)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Color;

    #[test]
    fn any_picture_becomes_the_square() {
        for (w, h) in [(1, 1), (3, 700), (5000, 40), (320, 320), (641, 2)] {
            let r = Raster::filled(w, h, Color::rgb(200, 100, 50)).unwrap();
            let v = input(&r, 320, ([0.0; 3], [1.0; 3])).unwrap();
            assert_eq!(v.len(), 3 * 320 * 320, "{w}×{h}");
            // Scaled by the brightest value: red is 1, green a half, blue a quarter.
            let red = v[0];
            let green = v[320 * 320 + 7];
            let blue = v[2 * 320 * 320 + 99];
            assert!((red - 1.0).abs() < 1e-3 && (green - 0.5).abs() < 1e-3 && (blue - 0.25).abs() < 1e-3, "{w}×{h}: {red} {green} {blue}");
        }
    }

    #[test]
    fn halves_stay_halves() {
        // Left half black, right half white, very long: the square keeps the halves.
        let mut r = Raster::filled(6000, 30, Color::BLACK).unwrap();
        for px in r.pixels_mut().as_chunks_mut::<4>().0.chunks_mut(6000).flat_map(|row| row[3000..].iter_mut()) {
            *px = [255, 255, 255, 255];
        }
        let v = input(&r, 64, ([0.0; 3], [1.0; 3])).unwrap();
        assert!(v[64 * 10 + 5] < 0.01 && v[64 * 10 + 60] > 0.99);
    }

    #[test]
    fn answers_become_mattes() {
        let m = to_matte(&[0.2, 0.6, f32::NAN, 1.0], 2).unwrap();
        assert_eq!(m.alpha(), &[0, 128, 255, 255]);
        let flat = to_matte(&[0.5; 4], 2).unwrap();
        assert!(flat.alpha().iter().all(|a| *a == 255), "nothing told apart, nothing taken");
        assert!(to_matte(&[0.5; 3], 2).is_err());
    }
}
