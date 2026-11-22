//! Colour changes to the picture, or to an area of it only: hue, saturation, lightness and
//! gamma; sepia; sharpen; blur. Greyscale, invert and brightness ([`crate::effects`]) are kept to
//! an area the same way ([`within`]).
//!
//! Each returns a new raster of the same size; nothing moves, so the annotations stay put.

use screengrabcraft_model::{Raster, RasterError};

use crate::{canvas, effects};

/// An area of a picture in whole pixels: x, y, width, height.
pub type Area = (u32, u32, u32, u32);

/// `f` done to the part of `r` in `area` only (all of it when there is none). The part is worked
/// on alone, its edges repeated (a blur doesn't reach in from outside it).
pub fn within(r: &Raster, area: Option<Area>, f: impl FnOnce(&Raster) -> Result<Raster, RasterError>) -> Result<Raster, RasterError> {
    let Some((x, y, w, h)) = area else { return f(r) };
    if (x, y, w, h) == (0, 0, r.width(), r.height()) {
        return f(r);
    }
    let part = r.crop(x, y, w, h).ok_or(RasterError::Empty(w, h))?;
    let done = f(&part)?;
    let mut out = r.clone();
    effects::paste(&mut out, &done, i64::from(x), i64::from(y));
    Ok(out)
}

/// A number made finite and kept between `lo` and `hi` (`default` for NaN).
fn bounded(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { default }
}

/// Red, green and blue (0 to 1) as hue (0 to 6, a sixth a turn each), saturation and lightness.
fn to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h, s, l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s <= f32::EPSILON {
        return (l, l, l);
    }
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h.rem_euclid(6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (r + m, g + m, b + m)
}

fn byte(v: f32) -> u8 {
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

/// The colours turned round the colour wheel by `hue` degrees (-180 to 180), made more or less
/// colourful by `saturation` (-100, grey, to 100), lighter or darker by `lightness` (-100, black,
/// to 100, white), and their mid-tones lightened or darkened by `gamma` (0.2 to 5; above 1
/// lightens). Alpha is kept.
pub fn hue_saturation(r: &Raster, hue: f32, saturation: f32, lightness: f32, gamma: f32) -> Raster {
    let turn = bounded(hue, -180.0, 180.0, 0.0) / 60.0;
    let sat = 1.0 + bounded(saturation, -100.0, 100.0, 0.0) / 100.0;
    let light = bounded(lightness, -100.0, 100.0, 0.0) / 100.0;
    let gamma = bounded(gamma, 0.2, 5.0, 1.0);
    let lut: Vec<u8> = (0..=255u32).map(|v| byte((v as f32 / 255.0).powf(1.0 / gamma))).collect();
    let colour = turn != 0.0 || sat != 1.0 || light != 0.0;
    let mut out = r.clone();
    for px in out.pixels_mut().as_chunks_mut::<4>().0 {
        if colour {
            let (h, s, l) = to_hsl(f32::from(px[0]) / 255.0, f32::from(px[1]) / 255.0, f32::from(px[2]) / 255.0);
            let s = (s * sat).clamp(0.0, 1.0);
            let l = if light >= 0.0 { l + (1.0 - l) * light } else { l * (1.0 + light) };
            let (nr, ng, nb) = from_hsl(h + turn, s, l.clamp(0.0, 1.0));
            px[0] = byte(nr);
            px[1] = byte(ng);
            px[2] = byte(nb);
        }
        for c in px.iter_mut().take(3) {
            *c = lut.get(usize::from(*c)).copied().unwrap_or(*c);
        }
    }
    out
}

/// The old-photograph brown, `amount` of the way (0 to 100) from the colours as they are.
pub fn sepia(r: &Raster, amount: f32) -> Raster {
    let k = bounded(amount, 0.0, 100.0, 100.0) / 100.0;
    let mut out = r.clone();
    for px in out.pixels_mut().as_chunks_mut::<4>().0 {
        let (red, green, blue) = (f32::from(px[0]), f32::from(px[1]), f32::from(px[2]));
        let toned =
            [0.393 * red + 0.769 * green + 0.189 * blue, 0.349 * red + 0.686 * green + 0.168 * blue, 0.272 * red + 0.534 * green + 0.131 * blue];
        for (c, t) in px.iter_mut().zip(toned) {
            *c = (f32::from(*c) + (t - f32::from(*c)) * k).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Blurred with a softness of `radius` pixels (0.5 to 100).
pub fn blur(r: &Raster, radius: f32) -> Result<Raster, RasterError> {
    let mut pix = canvas::to_pixmap(r).ok_or(RasterError::Empty(r.width(), r.height()))?;
    canvas::blur(&mut pix, bounded(radius, 0.5, 100.0, 4.0));
    canvas::from_pixmap(&pix).ok_or(RasterError::Empty(r.width(), r.height()))
}

/// Edges made crisper (an unsharp mask): each pixel pushed away from its blurred neighbourhood
/// (`radius`, 0.5 to 10 pixels) by `amount` (0 to 300, in hundredths).
pub fn sharpen(r: &Raster, amount: f32, radius: f32) -> Result<Raster, RasterError> {
    let k = bounded(amount, 0.0, 300.0, 60.0) / 100.0;
    let soft = blur(r, bounded(radius, 0.5, 10.0, 1.5))?;
    let mut out = r.clone();
    for (px, b) in out.pixels_mut().as_chunks_mut::<4>().0.iter_mut().zip(soft.pixels().as_chunks::<4>().0) {
        for (c, s) in px.iter_mut().zip(b.iter()).take(3) {
            let v = f32::from(*c);
            *c = (v + (v - f32::from(*s)) * k).round().clamp(0.0, 255.0) as u8;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::Color;

    use super::*;

    #[test]
    fn hue_turns_red_to_green_and_saturation_greys() {
        let red = Raster::filled(2, 2, Color::rgb(255, 0, 0)).unwrap();
        assert_eq!(hue_saturation(&red, 120.0, 0.0, 0.0, 1.0).pixel(0, 0), Some(Color::rgb(0, 255, 0)));
        let grey = hue_saturation(&red, 0.0, -100.0, 0.0, 1.0).pixel(0, 0).unwrap();
        assert!(grey.r == grey.g && grey.g == grey.b, "{grey:?}");
        assert_eq!(hue_saturation(&red, 0.0, 0.0, 100.0, 1.0).pixel(0, 0), Some(Color::WHITE));
        assert_eq!(hue_saturation(&red, 0.0, 0.0, -100.0, 1.0).pixel(0, 0), Some(Color::BLACK));
        // Gamma lightens the middle and keeps the ends.
        let mid = Raster::filled(1, 1, Color::rgba(128, 0, 255, 77)).unwrap();
        let g = hue_saturation(&mid, 0.0, 0.0, 0.0, 2.0).pixel(0, 0).unwrap();
        assert!(g.r > 170 && g.g == 0 && g.b == 255 && g.a == 77, "{g:?}");
        // Nothing asked, nothing changed; junk is nothing.
        let same = hue_saturation(&mid, f32::NAN, f32::INFINITY, 0.0, f32::NAN).pixel(0, 0).unwrap();
        assert!(same.r.abs_diff(128) <= 1 && same.b == 255, "{same:?}");
    }

    #[test]
    fn sepia_browns_and_sharpen_crisps() {
        let white = Raster::filled(3, 3, Color::rgb(200, 200, 200)).unwrap();
        let s = sepia(&white, 100.0).pixel(1, 1).unwrap();
        assert!(s.r > s.g && s.g > s.b, "brown: {s:?}");
        assert_eq!(sepia(&white, 0.0).pixel(1, 1), Some(Color::rgb(200, 200, 200)));
        // A dark line on grey gets darker, its sides lighter.
        let mut r = Raster::filled(9, 3, Color::rgb(150, 150, 150)).unwrap();
        for y in 0..3 {
            let i = (y * 9 + 4) * 4;
            r.pixels_mut()[i..i + 3].copy_from_slice(&[60, 60, 60]);
        }
        let sharp = sharpen(&r, 150.0, 1.0).unwrap();
        assert!(sharp.pixel(4, 1).unwrap().r < 60 && sharp.pixel(3, 1).unwrap().r > 150);
        let soft = blur(&r, 2.0).unwrap();
        assert!(soft.pixel(4, 1).unwrap().r > 60 && soft.pixel(3, 1).unwrap().r < 150);
    }

    #[test]
    fn an_area_alone_is_changed() {
        let r = Raster::filled(10, 10, Color::rgb(200, 20, 20)).unwrap();
        let out = within(&r, Some((2, 2, 3, 3)), |p| Ok(effects::invert(p))).unwrap();
        assert_eq!(out.pixel(3, 3), Some(Color::rgb(55, 235, 235)));
        assert_eq!(out.pixel(0, 0), Some(Color::rgb(200, 20, 20)));
        assert_eq!(out.pixel(5, 5), Some(Color::rgb(200, 20, 20)));
        assert!(within(&r, Some((20, 20, 3, 3)), |p| Ok(p.clone())).is_err(), "off the picture");
        assert_eq!(within(&r, None, |p| Ok(effects::invert(p))).unwrap().pixel(9, 9), Some(Color::rgb(55, 235, 235)));
    }
}
