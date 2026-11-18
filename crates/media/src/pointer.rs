//! The mouse pointer of a recording: where it was, which pointer it was and which buttons were
//! down, moment by moment, kept apart from the screen's pictures. So it can be drawn larger,
//! its path smoothed, its clicks shown, or left out, after the recording.
//!
//! ```text
//! PNTR  t:u64 (µs) x:f32 y:f32 (the screen track's pixels) flags:u8 shape:u16
//!         flags: 1 showing, 2 left button, 4 right, 8 middle
//! PSHP  id:u16 hot_x:u16 hot_y:u16 nominal:u16 width:u16 height:u16 lz4(RGBA)
//! ```
//!
//! Smoothing never moves a click: at the moment a button goes down or up, and all the while one
//! is held (a drag), the pointer is exactly where it was; between clicks its path is the
//! average of where it was a little before and after, eased in and out of those moments.
//!
//! Everything read is checked: at most [`MAX_SAMPLES`] samples and [`MAX_SHAPES`] pictures of
//! at most [`MAX_SIDE`] a side; what doesn't fit is left out.

use screengrabcraft_model::Raster;
use serde::{Deserialize, Serialize};

pub const MAX_SAMPLES: usize = 5_000_000;
pub const MAX_SHAPES: usize = 64;
pub const MAX_SIDE: u32 = 256;
/// How far either side of a moment the smoothest path looks (µs).
const SMOOTH_US: u64 = 280_000;
/// How long a click's ring shows (µs).
const RING_US: u64 = 450_000;

pub const SHOWING: u8 = 1;
pub const LEFT: u8 = 2;
pub const RIGHT: u8 = 4;
pub const MIDDLE: u8 = 8;
const BUTTONS: u8 = LEFT | RIGHT | MIDDLE;

/// The pointer at one moment of the recording.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    pub t: u64,
    /// In the screen track's pixels (it may be off the picture).
    pub x: f32,
    pub y: f32,
    pub flags: u8,
    /// Which picture ([`Shape::id`]); one that isn't there is the arrow.
    pub shape: u16,
}

/// A pointer's picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub id: u16,
    /// The pixel of the picture that points.
    pub hot: (u16, u16),
    /// How wide the picture is drawn on the screen track at its own size.
    pub nominal: u16,
    pub pic: Raster,
}

/// How the pointer is drawn on the video.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Look {
    pub show: bool,
    /// Times its own size: 1 to 4.
    pub size: f32,
    /// How much its path is smoothed: 0 (as it was) to 1.
    pub smooth: f32,
    /// A ring where a button is pressed.
    pub clicks: bool,
}

impl Default for Look {
    fn default() -> Look {
        Look { show: true, size: 1.0, smooth: 0.4, clicks: true }
    }
}

impl Look {
    pub fn checked(self) -> Look {
        let n = |v: f32, lo: f32, hi: f32, or: f32| if v.is_finite() { v.clamp(lo, hi) } else { or };
        Look { show: self.show, size: n(self.size, 0.5, 4.0, 1.0), smooth: n(self.smooth, 0.0, 1.0, 0.0), clicks: self.clicks }
    }
}

/// A button pressed: when, where, which.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Click {
    pub t: u64,
    pub x: f32,
    pub y: f32,
    pub button: u8,
}

/// A recording's pointer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    samples: Vec<Sample>,
    shapes: Vec<Shape>,
    clicks: Vec<Click>,
    /// The stretches a button was held (from its press to its release).
    held: Vec<(u64, u64)>,
}

/// A sample as its chunk's payload.
pub fn sample_bytes(s: &Sample) -> Vec<u8> {
    let mut b = Vec::with_capacity(19);
    b.extend_from_slice(&s.t.to_le_bytes());
    b.extend_from_slice(&s.x.to_le_bytes());
    b.extend_from_slice(&s.y.to_le_bytes());
    b.push(s.flags);
    b.extend_from_slice(&s.shape.to_le_bytes());
    b
}

pub fn sample_from(b: &[u8]) -> Option<Sample> {
    let f = |at: usize| Some(f32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?));
    let (x, y) = (f(8)?, f(12)?);
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    Some(Sample {
        t: u64::from_le_bytes(b.get(..8)?.try_into().ok()?),
        x: x.clamp(-1.0e6, 1.0e6),
        y: y.clamp(-1.0e6, 1.0e6),
        flags: *b.get(16)?,
        shape: u16::from_le_bytes(b.get(17..19)?.try_into().ok()?),
    })
}

/// A picture as its chunk's payload.
pub fn shape_bytes(s: &Shape) -> Vec<u8> {
    let mut b = Vec::new();
    for v in [s.id, s.hot.0, s.hot.1, s.nominal, s.pic.width() as u16, s.pic.height() as u16] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend(lz4_flex::compress_prepend_size(s.pic.pixels()));
    b
}

pub fn shape_from(b: &[u8]) -> Option<Shape> {
    let n = |at: usize| Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?));
    let (id, hx, hy, nominal, w, h) = (n(0)?, n(2)?, n(4)?, n(6)?, u32::from(n(8)?), u32::from(n(10)?));
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let body = b.get(12..)?;
    // The size it says it unpacks to is checked before any of it is.
    let says = u32::from_le_bytes(body.get(..4)?.try_into().ok()?);
    if says != w * h * 4 {
        return None;
    }
    let px = lz4_flex::decompress_size_prepended(body).ok()?;
    let pic = Raster::new(w, h, px).ok()?;
    Some(Shape { id, hot: (hx.min(w as u16 - 1), hy.min(h as u16 - 1)), nominal: nominal.clamp(8, MAX_SIDE as u16), pic })
}

/// Where the pointer is drawn at a moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub x: f32,
    pub y: f32,
    pub shape: u16,
}

impl Track {
    /// From what a recording holds (samples in any order; what's too much is left out).
    pub fn new(mut samples: Vec<Sample>, mut shapes: Vec<Shape>) -> Track {
        samples.truncate(MAX_SAMPLES);
        samples.sort_by_key(|s| s.t);
        shapes.truncate(MAX_SHAPES);
        let (mut clicks, mut held) = (Vec::new(), Vec::new());
        let mut down: Option<u64> = None;
        let mut before = 0u8;
        for s in &samples {
            let now = s.flags & BUTTONS;
            for button in [LEFT, RIGHT, MIDDLE] {
                if now & button != 0 && before & button == 0 {
                    clicks.push(Click { t: s.t, x: s.x, y: s.y, button });
                }
            }
            match (down, now != 0) {
                (None, true) => down = Some(s.t),
                (Some(from), false) => {
                    held.push((from, s.t));
                    down = None;
                }
                _ => {}
            }
            before = now;
        }
        if let (Some(from), Some(last)) = (down, samples.last()) {
            held.push((from, last.t));
        }
        Track { samples, shapes, clicks, held }
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    pub fn clicks(&self) -> &[Click] {
        &self.clicks
    }

    pub fn shape(&self, id: u16) -> Option<&Shape> {
        self.shapes.iter().find(|s| s.id == id)
    }

    /// Where it was at `t`, as recorded: on the line between the samples either side.
    fn raw(&self, t: u64) -> Option<(f32, f32)> {
        let i = self.samples.partition_point(|s| s.t <= t);
        let a = self.samples.get(i.checked_sub(1)?)?;
        let Some(b) = self.samples.get(i) else { return Some((a.x, a.y)) };
        let span = b.t.saturating_sub(a.t).max(1);
        // A long gap (it was paused over, say) isn't a glide.
        if span > 1_000_000 {
            return Some((a.x, a.y));
        }
        let f = (t - a.t) as f32 / span as f32;
        Some((a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f))
    }

    /// How far `t` is from the nearest moment a button went down or up (0 while one is held).
    fn since_a_click(&self, t: u64) -> u64 {
        let i = self.held.partition_point(|h| h.0 <= t);
        let before = i.checked_sub(1).and_then(|k| self.held.get(k)).map_or(u64::MAX, |h| t.saturating_sub(h.1));
        let after = self.held.get(i).map_or(u64::MAX, |h| h.0.saturating_sub(t));
        before.min(after)
    }

    /// Where the pointer is drawn at `t` (recording time), its path smoothed by `smooth` (0 to
    /// 1); none when it wasn't showing (or before the first sample).
    pub fn pose(&self, t: u64, smooth: f32) -> Option<Pose> {
        let i = self.samples.partition_point(|s| s.t <= t);
        let at = self.samples.get(i.checked_sub(1)?)?;
        if at.flags & SHOWING == 0 {
            return None;
        }
        let (rx, ry) = self.raw(t)?;
        let reach = (smooth.clamp(0.0, 1.0) * SMOOTH_US as f32) as u64;
        if reach < 1000 {
            return Some(Pose { x: rx, y: ry, shape: at.shape });
        }
        // The average of where it was from `reach` before to `reach` after, nearer moments
        // counting more: looked at more often than it was sampled, so a tremor at the sampling's
        // own beat averages out too.
        const TAPS: i64 = 32;
        let (mut sx, mut sy, mut sw) = (0f32, 0f32, 0f32);
        for k in -TAPS..=TAPS {
            let dt = reach as i64 * k / TAPS;
            let Some(tt) = (t as i64).checked_add(dt).filter(|v| *v >= 0) else { continue };
            let Some((x, y)) = self.raw(tt as u64) else { continue };
            let w = 1.0 - (k.unsigned_abs() as f32 / (TAPS + 1) as f32);
            sx += x * w;
            sy += y * w;
            sw += w;
        }
        if sw <= 0.0 {
            return Some(Pose { x: rx, y: ry, shape: at.shape });
        }
        // Eased out of every click and into the next: at a click it's exactly where it was.
        let near = (self.since_a_click(t) as f32 / reach as f32).clamp(0.0, 1.0);
        let k = near * near * (3.0 - 2.0 * near);
        Some(Pose { x: rx + (sx / sw - rx) * k, y: ry + (sy / sw - ry) * k, shape: at.shape })
    }

    /// The clicks whose rings show at `t`: each with how far through its ring it is (0 to 1).
    pub fn rings(&self, t: u64) -> Vec<(Click, f32)> {
        let from = self.clicks.partition_point(|c| c.t + RING_US <= t);
        self.clicks.get(from..).unwrap_or_default().iter().take_while(|c| c.t <= t).map(|c| (*c, (t - c.t) as f32 / RING_US as f32)).collect()
    }
}

/// The arrow drawn when a recording has no picture for a pointer (64 × 64, drawn at 32).
pub fn arrow() -> Option<Shape> {
    let clear = Raster::filled(64, 64, screengrabcraft_model::Color::TRANSPARENT).ok()?;
    let pic = screengrabcraft_render::pointer::draw(&clear, 3.0, 3.0, 2.0)?;
    Some(Shape { id: u16::MAX, hot: (3, 3), nominal: 32, pic })
}

fn blend(out: &mut [u8], c: [u8; 3], a: f32) {
    if a <= 0.0 {
        return;
    }
    let a = a.min(1.0);
    for (o, v) in out.iter_mut().zip(c) {
        *o = (f32::from(*o) + (f32::from(v) - f32::from(*o)) * a).round() as u8;
    }
}

/// A click's ring at `(cx, cy)`: `p` through its life (it grows and fades).
fn ring(out: &mut Raster, cx: f32, cy: f32, unit: f32, p: f32, button: u8) {
    let colour = match button {
        RIGHT => [0x2F, 0x9B, 0xFF],
        MIDDLE => [0xFF, 0xFF, 0xFF],
        _ => [0xFF, 0xD2, 0x1E],
    };
    let radius = unit * (0.35 + 0.75 * p);
    let thick = (unit * 0.12).max(1.5);
    let alpha = (1.0 - p).clamp(0.0, 1.0) * 0.85;
    let (w, h) = (out.width() as i64, out.height() as i64);
    let reach = radius + thick + 1.0;
    let (x0, x1) = (((cx - reach).floor() as i64).max(0), ((cx + reach).ceil() as i64).min(w - 1));
    let (y0, y1) = (((cy - reach).floor() as i64).max(0), ((cy + reach).ceil() as i64).min(h - 1));
    let px = out.pixels_mut();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            // Inside the ring a faint wash; on it, the ring.
            let on = (1.0 - ((d - radius).abs() - thick / 2.0)).clamp(0.0, 1.0);
            let wash = if d < radius { 0.18 } else { 0.0 };
            let i = ((y * w + x) * 4) as usize;
            if let Some(o) = px.get_mut(i..i + 3) {
                blend(o, colour, alpha * on.max(wash));
            }
        }
    }
}

/// A pointer's picture drawn with its hot spot at `(x, y)`, `f` times its own pixels.
fn stamp(out: &mut Raster, shape: &Shape, x: f32, y: f32, f: f32) {
    let (sw, sh) = (shape.pic.width() as f32, shape.pic.height() as f32);
    let f = f.clamp(0.05, 16.0);
    let (left, top) = (x - f32::from(shape.hot.0) * f, y - f32::from(shape.hot.1) * f);
    let (w, h) = (out.width() as i64, out.height() as i64);
    let (x0, x1) = ((left.floor() as i64).max(0), ((left + sw * f).ceil() as i64).min(w - 1));
    let (y0, y1) = ((top.floor() as i64).max(0), ((top + sh * f).ceil() as i64).min(h - 1));
    let src = shape.pic.pixels();
    let (swi, shi) = (shape.pic.width() as i64, shape.pic.height() as i64);
    let at = |sx: i64, sy: i64| -> [f32; 4] {
        if sx < 0 || sy < 0 || sx >= swi || sy >= shi {
            return [0.0; 4];
        }
        let i = ((sy * swi + sx) * 4) as usize;
        match src.get(i..i + 4) {
            // Premultiplied, so a clear pixel's colour doesn't bleed into the edge.
            Some(p) => {
                let a = f32::from(p[3]) / 255.0;
                [f32::from(p[0]) * a, f32::from(p[1]) * a, f32::from(p[2]) * a, a]
            }
            None => [0.0; 4],
        }
    };
    let px = out.pixels_mut();
    for oy in y0..=y1 {
        for ox in x0..=x1 {
            // The middle of this pixel, in the picture's.
            let (u, v) = ((ox as f32 + 0.5 - left) / f - 0.5, (oy as f32 + 0.5 - top) / f - 0.5);
            let (ux, vy) = (u.floor(), v.floor());
            let (fx, fy) = (u - ux, v - vy);
            let (ix, iy) = (ux as i64, vy as i64);
            let (a, b, c, d) = (at(ix, iy), at(ix + 1, iy), at(ix, iy + 1), at(ix + 1, iy + 1));
            let mix = |k: usize| (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
            let alpha = mix(3);
            if alpha <= 0.003 {
                continue;
            }
            let i = ((oy * w + ox) * 4) as usize;
            if let Some(o) = px.get_mut(i..i + 3) {
                for (k, o) in o.iter_mut().enumerate() {
                    *o = (f32::from(*o) * (1.0 - alpha) + mix(k)).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

/// Draws the pointer as it was at recording time `t` onto a picture of the screen track made
/// `scale` times its size: its clicks' rings, then the pointer itself.
pub fn draw(out: &mut Raster, track: &Track, look: Look, t: u64, scale: f32, fallback: Option<&Shape>) {
    let look = look.checked();
    if !look.show || track.is_empty() || !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let size = look.size * scale;
    if look.clicks {
        for (c, p) in track.rings(t) {
            ring(out, c.x * scale, c.y * scale, 34.0 * size, p, c.button);
        }
    }
    let Some(pose) = track.pose(t, look.smooth) else { return };
    let Some(shape) = track.shape(pose.shape).or(fallback) else { return };
    let f = f32::from(shape.nominal) * size / (shape.pic.width() as f32).max(1.0);
    stamp(out, shape, pose.x * scale, pose.y * scale, f);
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::Color;

    use super::*;

    fn s(t: u64, x: f32, y: f32, flags: u8) -> Sample {
        Sample { t, x, y, flags: flags | SHOWING, shape: 0 }
    }

    /// A pointer zig-zagging right (a shaky hand), with a click at 1 s.
    fn shaky() -> Track {
        let mut v = Vec::new();
        for k in 0..120u64 {
            let t = k * 16_667;
            let wobble = if k % 2 == 0 { 6.0 } else { -6.0 };
            let down = (60..66).contains(&k);
            v.push(s(t, 100.0 + k as f32 * 3.0, 200.0 + wobble, if down { LEFT } else { 0 }));
        }
        Track::new(v, Vec::new())
    }

    #[test]
    fn samples_and_pictures_come_back_as_written() {
        let a = Sample { t: 1_234_567, x: 10.5, y: -3.25, flags: SHOWING | RIGHT, shape: 7 };
        assert_eq!(sample_from(&sample_bytes(&a)), Some(a));
        let mut pic = Raster::filled(3, 2, Color::rgba(10, 20, 30, 40)).unwrap();
        pic.pixels_mut()[0] = 200;
        let sh = Shape { id: 7, hot: (2, 1), nominal: 32, pic };
        assert_eq!(shape_from(&shape_bytes(&sh)), Some(sh.clone()));
        // Junk: nothing, never a panic. A picture that claims a huge size isn't unpacked.
        for junk in [Vec::new(), vec![0u8; 5], vec![0xFF; 19], vec![0xFF; 64]] {
            let _ = sample_from(&junk);
            let _ = shape_from(&junk);
        }
        let mut nan = sample_bytes(&a);
        nan[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(sample_from(&nan).is_none());
        let mut big = shape_bytes(&sh);
        big[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(shape_from(&big).is_none());
    }

    #[test]
    fn smoothing_calms_the_path_but_a_click_stays_where_it_was() {
        let tr = shaky();
        assert_eq!(tr.clicks().len(), 1);
        let click = tr.clicks()[0];
        assert_eq!(click.t, 60 * 16_667);
        // As recorded it wobbles 6 either way; smoothed, hardly at all.
        let at = 30 * 16_667;
        let raw = tr.pose(at, 0.0).unwrap();
        assert!((raw.y - 206.0).abs() < 0.01, "{raw:?}");
        let smooth = tr.pose(at, 1.0).unwrap();
        assert!((smooth.y - 200.0).abs() < 1.5, "{smooth:?}");
        assert!((smooth.x - raw.x).abs() < 2.0, "it doesn't lag behind: {smooth:?} {raw:?}");
        // At the click, and all the while the button is held: exactly where it was.
        for k in 60..=65u64 {
            let t = k * 16_667;
            assert_eq!(tr.pose(t, 1.0), tr.pose(t, 0.0), "at {k}");
        }
        assert_eq!(tr.pose(click.t, 1.0).map(|p| (p.x, p.y)), Some((click.x, click.y)));
        // Eased back into the smooth path after it: no jump.
        let after = 66 * 16_667 + 8000;
        let (a, b) = (tr.pose(after, 1.0).unwrap(), tr.pose(after, 0.0).unwrap());
        assert!((a.y - b.y).abs() < 1.0 && (a.x - b.x).abs() < 1.0, "{a:?} {b:?}");
    }

    #[test]
    fn a_hidden_pointer_isnt_drawn_and_rings_fade() {
        let tr =
            Track::new(vec![s(0, 5.0, 5.0, 0), Sample { t: 500_000, x: 6.0, y: 6.0, flags: 0, shape: 0 }, s(900_000, 7.0, 7.0, LEFT)], Vec::new());
        assert!(tr.pose(100_000, 0.5).is_some());
        assert!(tr.pose(600_000, 0.5).is_none(), "hidden then");
        assert!(Track::default().pose(0, 0.5).is_none());
        assert!(tr.rings(800_000).is_empty());
        let r = tr.rings(900_000 + RING_US / 2);
        assert_eq!(r.len(), 1);
        assert!((r[0].1 - 0.5).abs() < 0.01);
        assert!(tr.rings(900_000 + RING_US).is_empty());
    }

    #[test]
    fn the_pointer_is_drawn_larger_when_asked_and_its_ring_where_it_clicked() {
        let arrow = arrow().unwrap();
        let tr = Track::new(vec![s(0, 100.0, 80.0, 0), s(1_000_000, 100.0, 80.0, LEFT), s(1_100_000, 100.0, 80.0, 0)], Vec::new());
        let white = || Raster::filled(320, 240, Color::WHITE).unwrap();
        let changed = |pic: &Raster| pic.pixels().chunks(4).filter(|p| p[0] != 255 || p[1] != 255 || p[2] != 255).count();
        let mut small = white();
        draw(&mut small, &tr, Look { clicks: false, ..Look::default() }, 100_000, 1.0, Some(&arrow));
        let mut large = white();
        draw(&mut large, &tr, Look { clicks: false, size: 3.0, ..Look::default() }, 100_000, 1.0, Some(&arrow));
        assert!(changed(&small) > 40, "{}", changed(&small));
        assert!(changed(&large) > changed(&small) * 6, "{} {}", changed(&large), changed(&small));
        // The tip is where it points, at any size: black at the hot spot, white just up and left.
        for pic in [&small, &large] {
            assert!(pic.pixel(101, 82).unwrap().r < 120, "{:?}", pic.pixel(101, 82));
            assert_eq!(pic.pixel(96, 76).unwrap().r, 255);
        }
        // Not shown: nothing drawn.
        let mut none = white();
        draw(&mut none, &tr, Look { show: false, ..Look::default() }, 100_000, 1.0, Some(&arrow));
        assert_eq!(changed(&none), 0);
        // A click's ring, around where it clicked.
        let mut clicked = white();
        draw(&mut clicked, &tr, Look::default(), 1_150_000, 1.0, Some(&arrow));
        let yellow = clicked.pixels().chunks(4).filter(|p| p[0] > 240 && p[2] < 200).count();
        assert!(yellow > 100, "{yellow}");
        // On a video half the size, it's drawn half the size, at half the place.
        let mut half = Raster::filled(160, 120, Color::WHITE).unwrap();
        draw(&mut half, &tr, Look { clicks: false, ..Look::default() }, 100_000, 0.5, Some(&arrow));
        assert!(changed(&half) > 8 && changed(&half) < changed(&small), "{}", changed(&half));
        let (hw, px) = (half.width() as usize, half.pixels());
        let inside = |i: usize| (49..59).contains(&(i % hw)) && (39..52).contains(&(i / hw));
        assert!(px.chunks(4).enumerate().all(|(i, p)| p[0] == 255 && p[1] == 255 || inside(i)), "by its tip, at half the place");
    }

    #[test]
    fn junk_looks_are_made_safe() {
        let l = Look { show: true, size: f32::NAN, smooth: 99.0, clicks: true }.checked();
        assert_eq!((l.size, l.smooth), (1.0, 1.0));
        let tr = shaky();
        let mut pic = Raster::filled(8, 8, Color::WHITE).unwrap();
        draw(&mut pic, &tr, Look { size: 1.0e9, ..Look::default() }, 0, f32::INFINITY, None);
        draw(&mut pic, &tr, Look::default(), u64::MAX, 1.0e9, arrow().as_ref());
    }
}
