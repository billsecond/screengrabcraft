//! Making the background transparent.
//!
//! A [`Matte`] says how much of each pixel is kept (255 all of it, 0 none), and [`apply`] puts
//! it into the picture's alpha. Two ways to get one are plain picture work and live here: the
//! colour round the picture's edges taken away up to the subject ([`remove_colour`] with
//! [`Region::Edges`], for screenshots on a plain colour), and one colour taken away anywhere
//! ([`Region::Everywhere`]). Both leave the pixels at the edge of what went partly transparent,
//! with the old background's colour taken out of them, so smooth edges stay smooth on any new
//! background.
//!
//! The third way, a model that tells the subject from the background, is the host's: loading a
//! model is I/O and running one takes seconds. A host lends one through [`Matting`]
//! (`screengrabcraft-matte` runs one in Rust).

use std::collections::HashMap;

use screengrabcraft_model::raster::checked_len;
use screengrabcraft_model::{Color, Raster, RasterError};

/// How much of each pixel of a picture is kept: 255 all of it, 0 none. One byte a pixel, rows
/// top to bottom.
#[derive(Clone, PartialEq, Eq)]
pub struct Matte {
    width: u32,
    height: u32,
    alpha: Vec<u8>,
}

impl std::fmt::Debug for Matte {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Matte({}×{})", self.width, self.height)
    }
}

impl Matte {
    pub fn new(width: u32, height: u32, alpha: Vec<u8>) -> Result<Matte, RasterError> {
        let want = checked_len(width, height)? / 4;
        if alpha.len() != want {
            return Err(RasterError::WrongLength { width, height, got: alpha.len(), want });
        }
        Ok(Matte { width, height, alpha })
    }

    /// Everything kept.
    pub fn opaque(width: u32, height: u32) -> Result<Matte, RasterError> {
        let len = checked_len(width, height)? / 4;
        Ok(Matte { width, height, alpha: vec![255; len] })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn alpha(&self) -> &[u8] {
        &self.alpha
    }

    /// How much of one pixel is kept, or none outside.
    pub fn get(&self, x: u32, y: u32) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.alpha.get(y as usize * self.width as usize + x as usize).copied()
    }

    /// The matte at another size, filtered bilinearly so soft edges stay soft.
    pub fn resized(&self, width: u32, height: u32) -> Result<Matte, RasterError> {
        let len = checked_len(width, height)? / 4;
        if (width, height) == (self.width, self.height) {
            return Ok(self.clone());
        }
        let (xs, ys) = (axis(self.width, width), axis(self.height, height));
        let sw = self.width as usize;
        let at = |x: usize, y: usize| f32::from(self.alpha.get(y * sw + x).copied().unwrap_or(0));
        let mut out = Vec::with_capacity(len);
        for &(y0, y1, ty) in &ys {
            for &(x0, x1, tx) in &xs {
                let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
                let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
                out.push((top + (bottom - top) * ty).round().clamp(0.0, 255.0) as u8);
            }
        }
        Matte::new(width, height, out)
    }
}

/// For each pixel along an axis `to` pixels long, the two pixels of the `from`-pixel axis it
/// falls between and how far it is from the first.
fn axis(from: u32, to: u32) -> Vec<(usize, usize, f32)> {
    let last = from.saturating_sub(1) as usize;
    let scale = from as f32 / to.max(1) as f32;
    (0..to)
        .map(|i| {
            let s = ((i as f32 + 0.5) * scale - 0.5).clamp(0.0, last as f32);
            let i0 = (s.floor() as usize).min(last);
            ((i0), (i0 + 1).min(last), s - i0 as f32)
        })
        .collect()
}

/// The picture with its alpha multiplied by the matte (which must be the picture's size).
pub fn apply(r: &Raster, m: &Matte) -> Result<Raster, RasterError> {
    if (r.width(), r.height()) != (m.width, m.height) {
        return Err(RasterError::WrongLength { width: r.width(), height: r.height(), got: m.alpha.len(), want: r.pixels().len() / 4 });
    }
    let mut out = r.clone();
    for (px, a) in out.pixels_mut().as_chunks_mut::<4>().0.iter_mut().zip(&m.alpha) {
        px[3] = ((u16::from(px[3]) * u16::from(*a) + 127) / 255) as u8;
    }
    Ok(out)
}

/// Why a matte couldn't be made.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MatteError {
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Model(String),
    #[error(transparent)]
    Picture(#[from] RasterError),
}

/// Something that tells a picture's subject from its background: a model the host loaded.
pub trait Matting: Send + Sync {
    /// Its name, as people see it and as the `model` parameter of `image.removeBackground` gives it.
    fn name(&self) -> &str;

    /// The picture's matte, the picture's size: 255 on the subject, 0 on the background, soft
    /// in between. `progress` hears how far it has got (0 to 1) and answers whether to go on;
    /// `false` cancels it ([`MatteError::Cancelled`]).
    fn matte(&self, picture: &Raster, progress: &mut dyn FnMut(f32) -> bool) -> Result<Matte, MatteError>;
}

/// How far a pixel's colour is from `bg`: the largest difference of a channel (alpha
/// included). A pixel that is already transparent counts as background.
fn distance(c: Color, bg: Color) -> u8 {
    if c.a == 0 {
        return 0;
    }
    c.r.abs_diff(bg.r).max(c.g.abs_diff(bg.g)).max(c.b.abs_diff(bg.b)).max(c.a.abs_diff(bg.a))
}

/// The pixels round the picture's edge, each once.
fn edge_pixels(r: &Raster) -> Vec<Color> {
    let (w, h) = (r.width(), r.height());
    let mut out = Vec::new();
    for x in 0..w {
        out.extend(r.pixel(x, 0));
        if h > 1 {
            out.extend(r.pixel(x, h - 1));
        }
    }
    for y in 1..h.saturating_sub(1) {
        out.extend(r.pixel(0, y));
        if w > 1 {
            out.extend(r.pixel(w - 1, y));
        }
    }
    out
}

fn average(colours: &[Color]) -> Option<Color> {
    let n = colours.len() as u64;
    if n == 0 {
        return None;
    }
    let mut sum = [0u64; 4];
    for c in colours {
        for (s, v) in sum.iter_mut().zip([c.r, c.g, c.b, c.a]) {
            *s += u64::from(v);
        }
    }
    let [r, g, b, a] = sum.map(|s| ((s + n / 2) / n).min(255) as u8);
    Some(Color::rgba(r, g, b, a))
}

/// The colour of the picture's background, if it has a plain one: the colour of most of its
/// edge (at least half of the edge pixels, give or take a little noise). None when the edges
/// are of many colours.
pub fn edge_colour(r: &Raster) -> Option<Color> {
    let edge = edge_pixels(r);
    // The commonest colour, to 5 bits a channel; transparent pixels are all one colour.
    let key = |c: &Color| if c.a == 0 { u32::MAX } else { u32::from_be_bytes([c.r >> 3, c.g >> 3, c.b >> 3, c.a >> 3]) };
    let mut bins: HashMap<u32, u32> = HashMap::new();
    for c in &edge {
        *bins.entry(key(c)).or_default() += 1;
    }
    let top = bins.into_iter().max_by_key(|&(k, n)| (n, std::cmp::Reverse(k))).map(|(k, _)| k)?;
    let first: Vec<Color> = edge.iter().filter(|c| key(c) == top).copied().collect();
    let centre = average(&first)?;
    let near: Vec<Color> = edge.iter().filter(|c| distance(**c, centre) <= 16).copied().collect();
    if near.len() * 2 < edge.len() {
        return None;
    }
    if centre.a == 0 {
        return Some(Color::TRANSPARENT);
    }
    average(&near)
}

/// Where [`remove_colour`] takes the colour away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// Where it reaches the picture's edge without crossing another colour: the background
    /// round a subject, not the same colour inside it.
    Edges,
    /// Wherever it is.
    Everywhere,
    /// Where it reaches this pixel without crossing another colour.
    From(u32, u32),
}

/// What [`remove_colour`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Removed {
    pub picture: Raster,
    /// Pixels made fully transparent.
    pub cleared: u64,
    /// Pixels at the edge of those made partly transparent.
    pub softened: u64,
}

/// How far (in pixels) a soft shadow is followed out from what was taken away.
const SHADOW_REACH: u16 = 64;
/// The darkest a shadow is taken to be (a mix with black beyond this is the subject's own).
const SHADOW_DARKEST: f32 = 0.8;

/// The picture with `colour` (within `tolerance`, 0 to 255 a channel) made transparent in
/// `region`. The pixels just outside what was taken away that are a mix of the colour and the
/// subject (the smooth edges of text and shapes) are made as transparent as they are mixed, and
/// the colour is taken out of them, so no fringe of the old background is left.
///
/// With `shadows`, a soft shadow the subject casts on the colour (the colour darkening bit by bit
/// towards the subject, as under a window) becomes a see-through black shadow, not a dark band of
/// the old colour.
///
/// The colour is compared with `colour` itself, not with the pixel next door, so a gradient
/// that starts at the background's colour is only taken as far as it stays within `tolerance`
/// (a grey one towards black on a light page, with `shadows`, looks like a shadow and becomes one).
pub fn remove_colour(r: &Raster, colour: Color, tolerance: u8, region: Region, shadows: bool) -> Result<Removed, RasterError> {
    let (w, h) = (r.width() as usize, r.height() as usize);
    let n = checked_len(r.width(), r.height())? / 4;
    let px = r.pixels();
    let colour_at = |i: usize| match px.get(i * 4..i * 4 + 4) {
        Some([r, g, b, a]) => Color::rgba(*r, *g, *b, *a),
        _ => Color::TRANSPARENT,
    };
    let near = |i: usize| distance(colour_at(i), colour) <= tolerance;
    // 1 where the colour is taken away.
    let mut gone = vec![0u8; n];
    match region {
        Region::Everywhere => {
            for (i, g) in gone.iter_mut().enumerate() {
                *g = u8::from(near(i));
            }
        }
        Region::Edges => {
            let mut seeds = Vec::new();
            for x in 0..w {
                seeds.push((x, 0));
                seeds.push((x, h - 1));
            }
            for y in 0..h {
                seeds.push((0, y));
                seeds.push((w - 1, y));
            }
            flood(w, h, &seeds, &near, &mut gone);
        }
        Region::From(x, y) => {
            if (x as usize) < w && (y as usize) < h {
                flood(w, h, &[(x as usize, y as usize)], &near, &mut gone);
            }
        }
    }
    let mut out = r.clone();
    let mut cleared = 0u64;
    let mut softened = 0u64;
    {
        let dst = out.pixels_mut();
        for (i, g) in gone.iter().enumerate() {
            if *g != 0
                && let Some(a) = dst.get_mut(i * 4 + 3)
            {
                *a = 0;
                cleared += 1;
            }
        }
        if colour.a == 255 {
            // The pixels next to what went (marked 2 once seen), then, for a soft shadow, on
            // outward from them while they stay a gradual mix of the colour and black.
            let is_gone = |gone: &[u8], x: usize, y: usize| gone.get(y * w + x).is_some_and(|g| *g == 1);
            let mut queue = std::collections::VecDeque::new();
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    if gone.get(i).is_none_or(|g| *g != 0) {
                        continue;
                    }
                    let touches =
                        (y.saturating_sub(1)..(y + 2).min(h)).any(|yy| (x.saturating_sub(1)..(x + 2).min(w)).any(|xx| is_gone(&gone, xx, yy)));
                    if touches {
                        queue.push_back((i, 0.0f32, 0u16));
                    }
                }
            }
            for (i, _, _) in &queue {
                if let Some(g) = gone.get_mut(*i) {
                    *g = 2;
                }
            }
            while let Some((i, from, ring)) = queue.pop_front() {
                let (x, y) = (i % w, i / w);
                let c = colour_at(i);
                if c.a == 0 {
                    continue;
                }
                let shadow = if shadows && ring < SHADOW_REACH { shadow_mix(c, colour).filter(|a| *a - from <= 0.2) } else { None };
                let soft = match shadow {
                    Some(a) => {
                        let next = [
                            x.checked_sub(1).map(|x| (x, y)),
                            (x + 1 < w).then_some((x + 1, y)),
                            y.checked_sub(1).map(|y| (x, y)),
                            (y + 1 < h).then_some((x, y + 1)),
                        ];
                        for (nx, ny) in next.into_iter().flatten() {
                            if let Some(g) = gone.get_mut(ny * w + nx)
                                && *g == 0
                            {
                                *g = 2;
                                queue.push_back((ny * w + nx, a, ring + 1));
                            }
                        }
                        Some((a, pure(c, colour, a)))
                    }
                    None if ring == 0 => unmix(r, &gone, x, y, colour),
                    None => None,
                };
                if let Some((a, c)) = soft
                    && let Some(p) = dst.get_mut(i * 4..i * 4 + 4)
                {
                    p[0] = c.r;
                    p[1] = c.g;
                    p[2] = c.b;
                    p[3] = (f32::from(p[3]) * a).round().clamp(0.0, 255.0) as u8;
                    softened += 1;
                }
            }
        }
    }
    Ok(Removed { picture: out, cleared, softened })
}

/// How dark a shadow of black on `bg` the colour `c` is (0 none, 1 black), if it is one: it lies
/// on the line from `bg` to black, not too dark. A background close to black shows no shadows.
fn shadow_mix(c: Color, bg: Color) -> Option<f32> {
    let b = [f32::from(bg.r), f32::from(bg.g), f32::from(bg.b)];
    let cb = [f32::from(c.r) - b[0], f32::from(c.g) - b[1], f32::from(c.b) - b[2]];
    let d = [-b[0], -b[1], -b[2]];
    let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if dd < 60.0 * 60.0 {
        return None;
    }
    let a = (cb[0] * d[0] + cb[1] * d[1] + cb[2] * d[2]) / dd;
    if !(a > 0.0 && a <= SHADOW_DARKEST) {
        return None;
    }
    let rest = [cb[0] - d[0] * a, cb[1] - d[1] * a, cb[2] - d[2] * a];
    let off = (rest[0] * rest[0] + rest[1] * rest[1] + rest[2] * rest[2]).sqrt();
    (off <= 0.2 * (cb[0] * cb[0] + cb[1] * cb[1] + cb[2] * cb[2]).sqrt() + 3.0).then_some(a)
}

/// The colour `c` would be without `bg` mixed into it, `a` being how much of it isn't `bg`.
fn pure(c: Color, bg: Color, a: f32) -> Color {
    let a = a.max(1.0 / 255.0);
    let one = |c: u8, b: u8| ((f32::from(c) - (1.0 - a) * f32::from(b)) / a).round().clamp(0.0, 255.0) as u8;
    Color::rgba(one(c.r, bg.r), one(c.g, bg.g), one(c.b, bg.b), c.a)
}

/// The most pixels at the soft edge of a matte [`apply_clean`] takes the background out of.
const MAX_CLEANED: usize = 16_000_000;

/// [`apply`], and the old background's colour taken out of the pixels the matte keeps only partly
/// (the background near each one is the pixels the matte drops within eight pixels), so the soft
/// edge carries no fringe of it onto a new background.
pub fn apply_clean(r: &Raster, m: &Matte) -> Result<Raster, RasterError> {
    let mut out = apply(r, m)?;
    let (w, h) = (r.width() as usize, r.height() as usize);
    let partial: Vec<usize> = m.alpha.iter().enumerate().filter(|(_, a)| (26..=242).contains(*a)).map(|(i, _)| i).take(MAX_CLEANED + 1).collect();
    if partial.len() > MAX_CLEANED {
        return Ok(out);
    }
    let src = r.pixels();
    let dst = out.pixels_mut();
    const REACH: usize = 8;
    for i in partial {
        let (x, y) = (i % w, i / w);
        let (mut sum, mut n) = ([0f32; 3], 0f32);
        for yy in (y.saturating_sub(REACH)..(y + REACH + 1).min(h)).step_by(2) {
            for xx in (x.saturating_sub(REACH)..(x + REACH + 1).min(w)).step_by(2) {
                let j = yy * w + xx;
                if m.alpha.get(j).is_some_and(|a| *a < 13)
                    && let Some([r, g, b, _]) = src.get(j * 4..j * 4 + 4)
                {
                    sum[0] += f32::from(*r);
                    sum[1] += f32::from(*g);
                    sum[2] += f32::from(*b);
                    n += 1.0;
                }
            }
        }
        let (Some(a), Some([cr, cg, cb, ca])) = (m.alpha.get(i), src.get(i * 4..i * 4 + 4)) else { continue };
        if n < 1.0 {
            continue;
        }
        let bg = Color::rgb((sum[0] / n).round() as u8, (sum[1] / n).round() as u8, (sum[2] / n).round() as u8);
        let c = pure(Color::rgba(*cr, *cg, *cb, *ca), bg, f32::from(*a) / 255.0);
        if let Some(p) = dst.get_mut(i * 4..i * 4 + 3) {
            p.copy_from_slice(&[c.r, c.g, c.b]);
        }
    }
    Ok(out)
}

/// Fills `gone` with the pixels that `near` takes in and that are joined (side by side, not
/// corner to corner) to one of the seeds, a row of pixels at a time so the work list stays short.
pub(crate) fn flood(w: usize, h: usize, seeds: &[(usize, usize)], near: &dyn Fn(usize) -> bool, gone: &mut [u8]) {
    let open = |gone: &[u8], i: usize| gone.get(i).is_some_and(|g| *g == 0) && near(i);
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for &seed in seeds {
        stack.push(seed);
        while let Some((x, y)) = stack.pop() {
            if x >= w || y >= h || !open(gone, y * w + x) {
                continue;
            }
            let row = y * w;
            let (mut left, mut right) = (x, x);
            while left > 0 && open(gone, row + left - 1) {
                left -= 1;
            }
            while right + 1 < w && open(gone, row + right + 1) {
                right += 1;
            }
            if let Some(span) = gone.get_mut(row + left..=row + right) {
                span.fill(1);
            }
            for ny in [y.checked_sub(1), (y + 1 < h).then_some(y + 1)].into_iter().flatten() {
                let nrow = ny * w;
                let mut i = left;
                while i <= right {
                    if open(gone, nrow + i) {
                        stack.push((i, ny));
                        while i <= right && open(gone, nrow + i) {
                            i += 1;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
        }
    }
}

/// How much of the pixel at `(x, y)` is subject rather than `bg`, and the subject's colour,
/// when it looks like a mix of the two: the pixel lies on the line from `bg` to a purer colour
/// close by (within two pixels). None when it doesn't (a solid colour of its own, such as a thin
/// border): it is kept as it is.
fn unmix(r: &Raster, gone: &[u8], x: usize, y: usize, bg: Color) -> Option<(f32, Color)> {
    let (w, h) = (r.width() as usize, r.height() as usize);
    let rgb = |c: Color| [f32::from(c.r), f32::from(c.g), f32::from(c.b)];
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let b = rgb(bg);
    let cb = sub(rgb(r.pixel(x as u32, y as u32)?), b);
    let own = dot(cb, cb);
    let mut best: Option<f32> = None;
    for yy in y.saturating_sub(2)..(y + 3).min(h) {
        for xx in x.saturating_sub(2)..(x + 3).min(w) {
            if (xx, yy) == (x, y) || gone.get(yy * w + xx).is_some_and(|g| *g == 1) {
                continue;
            }
            let Some(q) = r.pixel(xx as u32, yy as u32) else { continue };
            if q.a == 0 {
                continue;
            }
            let fb = sub(rgb(q), b);
            let far = dot(fb, fb);
            if far <= own || far < 1.0 {
                continue;
            }
            let a = dot(cb, fb) / far;
            if !(0.0..1.0).contains(&a) || best.is_some_and(|best| a >= best) {
                continue;
            }
            let rest = sub(cb, [fb[0] * a, fb[1] * a, fb[2] * a]);
            if dot(rest, rest).sqrt() > 0.1 * far.sqrt() + 4.0 {
                continue;
            }
            best = Some(a);
        }
    }
    let a = best?;
    Some((a, pure(r.pixel(x as u32, y as u32)?, bg, a)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: Color = Color::rgb(30, 90, 200);

    /// A red disc, smoothly edged, on a plain blue background.
    fn disc_on_blue() -> Raster {
        let mut pix = crate::canvas::blank(80, 60).unwrap();
        pix.fill(tiny_skia::Color::from_rgba8(BLUE.r, BLUE.g, BLUE.b, 255));
        let path = tiny_skia::PathBuilder::from_circle(40.0, 30.0, 20.0).unwrap();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(229, 57, 53, 255);
        paint.anti_alias = true;
        pix.fill_path(&path, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
        crate::canvas::from_pixmap(&pix).unwrap()
    }

    #[test]
    fn a_matte_resizes_softly_and_applies() {
        let m = Matte::new(2, 1, vec![0, 255]).unwrap();
        let big = m.resized(8, 2).unwrap();
        assert_eq!((big.width(), big.height()), (8, 2));
        assert_eq!(big.get(0, 0), Some(0));
        assert_eq!(big.get(7, 1), Some(255));
        assert!(big.get(4, 0).is_some_and(|a| a > 0 && a < 255), "soft in the middle");
        assert!(Matte::new(2, 2, vec![0; 3]).is_err());
        assert!(Matte::new(0, 2, vec![]).is_err());
        let r = Raster::filled(8, 2, Color::RED).unwrap();
        let cut = apply(&r, &big).unwrap();
        assert_eq!(cut.pixel(0, 0).map(|c| c.a), Some(0));
        assert_eq!(cut.pixel(7, 0), Some(Color::RED));
        assert!(apply(&r, &m).is_err(), "sizes must match");
        let one = Matte::new(1, 1, vec![128]).unwrap().resized(3, 3).unwrap();
        assert!(one.alpha().iter().all(|a| *a == 128));
    }

    #[test]
    fn the_edge_colour_is_the_plain_background() {
        assert_eq!(edge_colour(&disc_on_blue()), Some(BLUE));
        assert_eq!(edge_colour(&Raster::single(Color::RED)), Some(Color::RED));
        // A quarter each of four colours: no plain background.
        let mut r = Raster::filled(10, 10, Color::WHITE).unwrap();
        for y in 0..10 {
            for x in 0..10 {
                let c = [[0, 0, 0, 255], [255, 0, 0, 255], [0, 255, 0, 255], [255, 255, 255, 255]][(y / 5) * 2 + x / 5];
                let i = (y * 10 + x) * 4;
                r.pixels_mut()[i..i + 4].copy_from_slice(&c);
            }
        }
        assert_eq!(edge_colour(&r), None);
        let mut stripes = Raster::filled(10, 10, Color::WHITE).unwrap();
        for (i, px) in stripes.pixels_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let v = ((i * 37) % 251) as u8;
            *px = [v, v.wrapping_mul(3), v.wrapping_mul(7), 255];
        }
        assert_eq!(edge_colour(&stripes), None, "noise has no plain background");
    }

    #[test]
    fn a_shape_on_a_plain_colour_comes_away_clean() {
        let r = disc_on_blue();
        let out = remove_colour(&r, BLUE, 24, Region::Edges, true).unwrap();
        let p = &out.picture;
        assert_eq!(p.pixel(0, 0).map(|c| c.a), Some(0), "the background is gone");
        assert_eq!(p.pixel(79, 59).map(|c| c.a), Some(0));
        assert_eq!(p.pixel(40, 30), Some(Color::rgb(229, 57, 53)), "the subject stays");
        assert!(out.cleared > 2000 && out.softened > 40, "{out:?}");
        // The smooth edge stays smooth, without a blue fringe.
        let mut partial = 0;
        for y in 0..60 {
            for x in 0..80 {
                let c = p.pixel(x, y).unwrap();
                if c.a > 0 && c.a < 255 {
                    partial += 1;
                    assert!(c.b < 120 && c.r > 150, "a fringe of the old background at {x},{y}: {c:?}");
                }
            }
        }
        assert!(partial > 40, "{partial} soft edge pixels");
    }

    /// A white page with a band fading from white to `to`, left to right.
    fn band_to(to: [u8; 3]) -> Raster {
        let mut r = Raster::filled(120, 60, Color::WHITE).unwrap();
        for y in 20..40u32 {
            for x in 10..110u32 {
                let t = (x - 10) as f32 / 99.0;
                let v = to.map(|c| (255.0 + (f32::from(c) - 255.0) * t).round() as u8);
                let i = ((y * 120 + x) * 4) as usize;
                r.pixels_mut()[i..i + 4].copy_from_slice(&[v[0], v[1], v[2], 255]);
            }
        }
        r
    }

    #[test]
    fn a_gradient_is_not_eaten() {
        // Only the part that is still nearly white goes; the rest of the band stays as it was.
        let red = remove_colour(&band_to([229, 57, 53]), Color::WHITE, 24, Region::Edges, true).unwrap();
        let grey = remove_colour(&band_to([0, 0, 0]), Color::WHITE, 24, Region::Edges, false).unwrap();
        for out in [red, grey] {
            let solid = (10..110u32).filter(|x| out.picture.pixel(*x, 30).is_some_and(|c| c.a == 255)).count();
            assert!(solid >= 85, "only {solid} of 100 columns of the band are left as they were");
            assert!((10..15u32).all(|x| out.picture.pixel(x, 30).is_some_and(|c| c.a < 255)), "the nearly white end goes");
        }
        // Towards black, with shadows on, it looks like a shadow and becomes one; the dark end stays.
        let shadow = remove_colour(&band_to([0, 0, 0]), Color::WHITE, 24, Region::Edges, true).unwrap();
        assert!(shadow.picture.pixel(50, 30).is_some_and(|c| c.a > 60 && c.a < 255 && c.r < 10));
        assert_eq!(shadow.picture.pixel(108, 30).map(|c| c.a), Some(255));
    }

    #[test]
    fn a_soft_shadow_stays_a_shadow() {
        // A white card with a soft black shadow, on a plain blue page.
        let mut pix = crate::canvas::blank(120, 90).unwrap();
        pix.fill(tiny_skia::Color::from_rgba8(BLUE.r, BLUE.g, BLUE.b, 255));
        let mut shadow = crate::canvas::blank(120, 90).unwrap();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(0, 0, 0, 140);
        let card = tiny_skia::Rect::from_xywh(30.0, 20.0, 60.0, 45.0).unwrap();
        shadow.fill_rect(tiny_skia::Rect::from_xywh(34.0, 26.0, 60.0, 45.0).unwrap(), &paint, tiny_skia::Transform::identity(), None);
        crate::canvas::blur(&mut shadow, 6.0);
        pix.draw_pixmap(0, 0, shadow.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        paint.set_color_rgba8(250, 250, 250, 255);
        pix.fill_rect(card, &paint, tiny_skia::Transform::identity(), None);
        let r = crate::canvas::from_pixmap(&pix).unwrap();
        let out = remove_colour(&r, BLUE, 16, Region::Edges, true).unwrap();
        // Below the card: see-through black, not a band of dark blue.
        let below = out.picture.pixel(60, 68).unwrap();
        assert!(below.a > 30 && below.a < 255 && below.b < 40, "{below:?}");
        assert_eq!(out.picture.pixel(60, 40), Some(Color::rgb(250, 250, 250)), "the card is untouched");
        assert_eq!(out.picture.pixel(2, 2).map(|c| c.a), Some(0));
        // Without shadows the dark blue stays as it was.
        let kept = remove_colour(&r, BLUE, 16, Region::Edges, false).unwrap();
        assert!(kept.picture.pixel(60, 68).is_some_and(|c| c.a == 255 && c.b > 100));
    }

    #[test]
    fn a_clean_matte_leaves_no_fringe() {
        // A red square on blue, its matte soft at the edge: the soft edge is red, not purple.
        let mut r = Raster::filled(40, 40, BLUE).unwrap();
        let mut alpha = vec![0u8; 1600];
        for y in 0..40 {
            for x in 0..40 {
                let inside = (10..30).contains(&x) && (10..30).contains(&y);
                let edge = (9..31).contains(&x) && (9..31).contains(&y) && !inside;
                let i = y * 40 + x;
                if inside {
                    r.pixels_mut()[i * 4..i * 4 + 4].copy_from_slice(&[229, 57, 53, 255]);
                    alpha[i] = 255;
                } else if edge {
                    // Half red, half blue, as the edge of a photo is.
                    r.pixels_mut()[i * 4..i * 4 + 4].copy_from_slice(&[130, 74, 127, 255]);
                    alpha[i] = 128;
                }
            }
        }
        let m = Matte::new(40, 40, alpha).unwrap();
        let plain = apply(&r, &m).unwrap();
        let clean = apply_clean(&r, &m).unwrap();
        assert_eq!(plain.pixel(9, 20), Some(Color::rgba(130, 74, 127, 128)), "a purple fringe");
        let c = clean.pixel(9, 20).unwrap();
        assert!(c.a == 128 && c.r > 200 && c.b < 80, "{c:?}");
        assert_eq!(clean.pixel(20, 20), Some(Color::rgb(229, 57, 53)));
        assert_eq!(clean.pixel(0, 0).map(|c| c.a), Some(0));
    }

    #[test]
    fn tolerance_sets_how_close_a_colour_must_be() {
        // A white page with faint noise (250 to 255), a black square in the middle.
        let mut r = Raster::filled(40, 40, Color::WHITE).unwrap();
        for (i, px) in r.pixels_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let v = 250 + (i * 7 % 6) as u8;
            *px = [v, v, v, 255];
        }
        for y in 15..25 {
            for x in 15..25 {
                let i = (y * 40 + x) * 4;
                r.pixels_mut()[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
        let exact = remove_colour(&r, Color::WHITE, 0, Region::Edges, true).unwrap();
        let loose = remove_colour(&r, Color::WHITE, 8, Region::Edges, true).unwrap();
        assert!(exact.cleared < loose.cleared);
        assert_eq!(loose.cleared, 40 * 40 - 100, "all of the noisy page");
        assert_eq!(loose.picture.pixel(20, 20), Some(Color::BLACK));
        let all = remove_colour(&r, Color::WHITE, 255, Region::Edges, true).unwrap();
        assert_eq!(all.cleared, 40 * 40, "everything is within 255");
    }

    #[test]
    fn holes_stay_unless_everywhere() {
        // A black ring on white: the white inside isn't reached from the edges.
        let mut r = Raster::filled(30, 30, Color::WHITE).unwrap();
        for y in 5..25 {
            for x in 5..25 {
                if !(10..20).contains(&x) || !(10..20).contains(&y) {
                    let i = (y * 30 + x) * 4;
                    r.pixels_mut()[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
        let edges = remove_colour(&r, Color::WHITE, 10, Region::Edges, true).unwrap();
        assert_eq!(edges.picture.pixel(15, 15), Some(Color::WHITE));
        let everywhere = remove_colour(&r, Color::WHITE, 10, Region::Everywhere, true).unwrap();
        assert_eq!(everywhere.picture.pixel(15, 15).map(|c| c.a), Some(0));
        let inside = remove_colour(&r, Color::WHITE, 10, Region::From(15, 15), true).unwrap();
        assert_eq!(inside.picture.pixel(15, 15).map(|c| c.a), Some(0));
        assert_eq!(inside.picture.pixel(0, 0), Some(Color::WHITE));
        let outside = remove_colour(&r, Color::WHITE, 10, Region::From(500, 500), true).unwrap();
        assert_eq!(outside.cleared, 0);
    }

    #[test]
    fn a_thin_border_of_its_own_colour_is_kept_solid() {
        // A white panel with a one-pixel grey border on a blue page: the border is no mix of
        // blue and white, so it stays solid.
        let mut r = Raster::filled(30, 30, BLUE).unwrap();
        for y in 5..25 {
            for x in 5..25 {
                let c = if x == 5 || x == 24 || y == 5 || y == 24 { [128, 128, 128, 255] } else { [255, 255, 255, 255] };
                let i = (y * 30 + x) * 4;
                r.pixels_mut()[i..i + 4].copy_from_slice(&c);
            }
        }
        let out = remove_colour(&r, BLUE, 24, Region::Edges, true).unwrap();
        assert_eq!(out.picture.pixel(5, 12), Some(Color::rgb(128, 128, 128)));
        assert_eq!(out.picture.pixel(4, 12).map(|c| c.a), Some(0));
    }

    #[test]
    fn tiny_pictures_work() {
        let one = Raster::single(Color::RED);
        let out = remove_colour(&one, Color::RED, 0, Region::Edges, true).unwrap();
        assert_eq!(out.cleared, 1);
        let out = remove_colour(&one, Color::BLACK, 0, Region::Everywhere, true).unwrap();
        assert_eq!(out.cleared, 0);
        let line = Raster::filled(1, 5, Color::WHITE).unwrap();
        assert_eq!(remove_colour(&line, Color::WHITE, 0, Region::Edges, true).unwrap().cleared, 5);
        let transparent = Raster::filled(3, 3, Color::TRANSPARENT).unwrap();
        assert_eq!(edge_colour(&transparent), Some(Color::TRANSPARENT));
    }
}
