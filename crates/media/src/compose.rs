//! What the edited video shows and plays at a moment: the screen's picture with the camera's
//! on top in a corner and the captions at the bottom ([`Composer`]), and the sound tracks mixed
//! ([`mix`]).

use std::sync::Arc;

use screengrabcraft_model::Raster;

use crate::captions::{self, Cue, Speaker};
use crate::recording::{FrameEntry, Recording, ScreenDecoder, StreamKind};
use crate::timeline::{CameraShape, Corner, Timeline};

/// Samples a second.
pub const RATE: u64 = 48_000;

/// A camera picture older than this (µs) isn't shown: the camera had stopped sending, and a
/// picture frozen in a corner would be wrong.
const CAMERA_STALE_US: u64 = 2_000_000;

/// µs to a sample number at 48 kHz.
pub fn to_samples(us: u64) -> u64 {
    us.saturating_mul(6) / 125
}

/// A sample number at 48 kHz to µs.
pub fn to_us(samples: u64) -> u64 {
    samples.saturating_mul(125) / 6
}

/// Builds the edited video's pictures, one moment at a time. Playing forward is cheap: only
/// what changed since the last picture is read.
pub struct Composer {
    rec: Arc<Recording>,
    screen: Option<(usize, ScreenDecoder)>,
    camera: Option<(usize, u8)>,
    /// The last camera picture decoded, and which it was.
    camera_frame: Option<(usize, Raster)>,
    /// Captions drawn on the picture (in timeline time), if any.
    captions: Option<Arc<Vec<Cue>>>,
    pub width: u32,
    pub height: u32,
}

impl Composer {
    /// `width × height` is the video's size (the screen's, or smaller).
    pub fn new(rec: Arc<Recording>, timeline: &Timeline, width: u32, height: u32) -> Composer {
        let screen = timeline.find(StreamKind::Screen).and_then(|i| timeline.tracks.get(i).map(|t| (i, ScreenDecoder::new(t.stream))));
        let camera = timeline.find(StreamKind::Camera).and_then(|i| timeline.tracks.get(i).map(|t| (i, t.stream)));
        Composer { rec, screen, camera, camera_frame: None, captions: None, width: width.max(2), height: height.max(2) }
    }

    /// The picture at timeline time `t` µs. A hidden track (`muted`) isn't in it: without the
    /// screen the camera is the picture.
    pub fn frame(&mut self, timeline: &Timeline, t: u64) -> Raster {
        let (w, h) = (self.width, self.height);
        let black =
            || Raster::filled(w, h, screengrabcraft_model::Color::BLACK).unwrap_or_else(|_| Raster::single(screengrabcraft_model::Color::BLACK));
        let shown = |i: usize| timeline.tracks.get(i).is_some_and(|tr| !tr.muted);
        let screen_shown = self.screen.as_ref().is_some_and(|(i, _)| shown(*i));
        let mut out = match &mut self.screen {
            Some((i, dec)) if screen_shown => timeline
                .source_time(*i, t)
                .and_then(|src| dec.frame_at(&self.rec, src))
                .and_then(|pic| if (pic.width(), pic.height()) == (w, h) { Some(pic) } else { scale(&pic, w, h) })
                .unwrap_or_else(black),
            _ => black(),
        };
        if let Some((i, stream)) = self.camera
            && shown(i)
            && let Some(src) = timeline.source_time(i, t)
            && let Some(cam) = self.camera_at(stream, src)
        {
            if screen_shown {
                overlay_camera(&mut out, &cam, timeline);
            } else {
                // Recorded (or shown) without the screen: the camera is the picture.
                fill_camera(&mut out, &cam);
            }
        }
        if let Some(cues) = &self.captions {
            draw_captions(&mut out, &captions::showing(cues, t));
        }
        out
    }

    /// Captions to draw on the pictures (in timeline time, see
    /// [`captions::Captions::on_timeline`]), or none.
    pub fn set_captions(&mut self, cues: Option<Arc<Vec<Cue>>>) {
        self.captions = cues;
    }

    /// The camera's picture at `src`: the last one up to then, unless the camera had stopped
    /// sending (none for over two seconds), when there's none.
    fn camera_at(&mut self, stream: u8, src: u64) -> Option<Raster> {
        let frames: &[FrameEntry] = self.rec.frames(stream);
        let i = frames.partition_point(|f| f.pts <= src).checked_sub(1)?;
        if frames.get(i).is_some_and(|f| src.saturating_sub(f.pts) > CAMERA_STALE_US) {
            return None;
        }
        if let Some((at, pic)) = &self.camera_frame
            && *at == i
        {
            return Some(pic.clone());
        }
        let jpeg = self.rec.jpeg(frames.get(i)?).ok()?;
        let pic = screengrabcraft_render::decode(&jpeg).ok()?;
        self.camera_frame = Some((i, pic.clone()));
        Some(pic)
    }
}

/// The size a video of this picture is made at: no larger than `max_w × max_h`, sides even.
pub fn fit(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let f = (f64::from(max_w) / f64::from(width.max(1))).min(f64::from(max_h) / f64::from(height.max(1))).min(1.0);
    let even = |v: f64| (((v.round() as u32).max(16)) / 2) * 2;
    (even(f64::from(width) * f), even(f64::from(height) * f))
}

/// Resizes by averaging the pixels each new one covers (sharp text stays readable when made
/// smaller), or by taking the nearest when made larger.
pub fn scale(src: &Raster, w: u32, h: u32) -> Option<Raster> {
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    let (w, h) = (w as usize, h as usize);
    if sw == 0 || sh == 0 || w == 0 || h == 0 {
        return None;
    }
    let px = src.pixels();
    let span = |i: usize, n: usize, sn: usize| {
        let a = i * sn / n;
        let b = ((i + 1) * sn / n).max(a + 1).min(sn);
        (a, b)
    };
    let cols: Vec<(usize, usize)> = (0..w).map(|x| span(x, w, sw)).collect();
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let (y0, y1) = span(y, h, sh);
        for &(x0, x1) in &cols {
            let mut acc = [0u32; 4];
            for sy in y0..y1 {
                let row = px.get(sy * sw * 4 + x0 * 4..sy * sw * 4 + x1 * 4).unwrap_or_default();
                for p in row.as_chunks::<4>().0 {
                    for (a, v) in acc.iter_mut().zip(p) {
                        *a += u32::from(*v);
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)).max(1) as u32;
            out.extend(acc.iter().map(|a| ((a + n / 2) / n) as u8));
        }
    }
    Raster::new(w as u32, h as u32, out).ok()
}

/// How much of a pixel at `(x, y)` (from the top left of a `w × h` box) the camera's shape
/// covers, 0 to 1, smoothed over a pixel at the edge.
fn coverage(shape: CameraShape, x: f32, y: f32, w: f32, h: f32) -> f32 {
    let (cx, cy) = (x + 0.5, y + 0.5);
    match shape {
        CameraShape::Rectangle => 1.0,
        CameraShape::Circle => {
            let r = w.min(h) / 2.0;
            let d = ((cx - w / 2.0).powi(2) + (cy - h / 2.0).powi(2)).sqrt();
            (r - d + 0.5).clamp(0.0, 1.0)
        }
        CameraShape::Rounded => {
            let r = w.min(h) * 0.12;
            let qx = (cx - w / 2.0).abs() - (w / 2.0 - r);
            let qy = (cy - h / 2.0).abs() - (h / 2.0 - r);
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
            (0.5 - outside).clamp(0.0, 1.0)
        }
    }
}

/// The colour of a speaker's captions on the picture: the others' white, yours yellow.
pub fn caption_colour(speaker: Speaker) -> screengrabcraft_model::Color {
    match speaker {
        Speaker::Others => screengrabcraft_model::Color::WHITE,
        Speaker::You => screengrabcraft_model::Color::rgb(0xFF, 0xE4, 0x5C),
    }
}

/// The most lines of a caption shown on the picture.
const CAPTION_LINES: usize = 3;

/// Draws caption lines near the bottom of the picture, centred, each on a dark band (the
/// others' above yours when both speak).
pub fn draw_captions(out: &mut Raster, lines: &[&Cue]) {
    use screengrabcraft_model::{Pt, Rect, Style, TextAlign};
    use screengrabcraft_render::{draw, text};
    if lines.is_empty() || out.width() < 16 || out.height() < 16 {
        return;
    }
    let (w, h) = (out.width() as f32, out.height() as f32);
    let size = (h * 0.045).clamp(10.0, 96.0);
    let (pad_x, pad_y, gap) = (size * 0.45, size * 0.18, size * 0.3);
    let mut blocks = Vec::with_capacity(lines.len());
    for c in lines.iter().take(2) {
        let style = Style { font_size: size, bold: true, text_color: caption_colour(c.speaker), align: TextAlign::Center, ..Style::default() };
        let mut lay = text::layout(&c.text, &style, Some(w * 0.86 - 2.0 * pad_x));
        lay.lines.truncate(CAPTION_LINES);
        lay.width = lay.lines.iter().map(|l| l.width).fold(0.0, f32::max);
        lay.height = lay.line_height * lay.lines.len() as f32;
        if !lay.lines.is_empty() && lay.width > 0.0 {
            blocks.push((lay, style));
        }
    }
    let total: f32 = blocks.iter().map(|(l, _)| l.height + 2.0 * pad_y).sum::<f32>() + gap * blocks.len().saturating_sub(1) as f32;
    let bottom = h - (h * 0.06).max(4.0);
    let top = (bottom - total).max(0.0);
    // Only the rows the captions cover are drawn on.
    let (y0, y1) = (top.floor() as u32, (bottom.ceil() as u32).min(out.height()));
    let row = out.width() as usize * 4;
    let (from, to) = (y0 as usize * row, y1 as usize * row);
    let Some(band) = out.pixels().get(from..to).map(<[u8]>::to_vec) else { return };
    let Some(size_px) = tiny_skia::IntSize::from_wh(out.width(), y1.saturating_sub(y0)) else { return };
    let Some(mut canvas) = tiny_skia::Pixmap::from_vec(band, size_px) else { return };
    let shift = tiny_skia::Transform::from_translate(0.0, -(y0 as f32));
    let mut y = top;
    for (lay, style) in &blocks {
        let bw = lay.width + 2.0 * pad_x;
        let bh = lay.height + 2.0 * pad_y;
        let x = ((w - bw) / 2.0).max(0.0);
        if let Some(back) = draw::rounded_rect(Rect::new(x, y, bw, bh), size * 0.22) {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color(tiny_skia::Color::from_rgba8(0, 0, 0, 165));
            paint.anti_alias = true;
            canvas.fill_path(&back, &paint, tiny_skia::FillRule::Winding, shift, None);
        }
        if let Some(path) = text::text_path(lay, style, Pt::new(x + pad_x, y + pad_y), lay.width) {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color(draw::skia_color(style.text_color, 1.0));
            paint.anti_alias = true;
            canvas.fill_path(&path, &paint, tiny_skia::FillRule::Winding, shift, None);
        }
        y += bh + gap;
    }
    if let Some(dst) = out.pixels_mut().get_mut(from..to) {
        dst.copy_from_slice(canvas.data());
    }
}

/// Draws the camera's picture over the whole of `out`, as large as fits, keeping its shape (a
/// recording with no screen).
pub fn fill_camera(out: &mut Raster, cam: &Raster) {
    let (ow, oh) = (out.width(), out.height());
    let f = (f64::from(ow) / f64::from(cam.width().max(1))).min(f64::from(oh) / f64::from(cam.height().max(1)));
    let cw = ((f64::from(cam.width()) * f).round() as u32).clamp(1, ow.max(1));
    let ch = ((f64::from(cam.height()) * f).round() as u32).clamp(1, oh.max(1));
    let Some(small) = (if (cw, ch) == (cam.width(), cam.height()) { Some(cam.clone()) } else { scale(cam, cw, ch) }) else { return };
    let (x0, y0) = ((ow.saturating_sub(cw) / 2) as usize, (oh.saturating_sub(ch) / 2) as usize);
    let (sw, ow_px) = (small.width() as usize, ow as usize);
    if sw == 0 {
        return;
    }
    let dst = out.pixels_mut();
    for (y, row) in small.pixels().chunks_exact(sw * 4).enumerate() {
        let i = ((y0 + y) * ow_px + x0) * 4;
        if let Some(d) = dst.get_mut(i..i + row.len()) {
            d.copy_from_slice(row);
        }
    }
}

/// Draws the camera's picture in its corner of `out`, in its shape.
pub fn overlay_camera(out: &mut Raster, cam: &Raster, timeline: &Timeline) {
    let layout = timeline.camera;
    let (ow, oh) = (out.width() as f32, out.height() as f32);
    let margin = (ow * 0.025).round();
    let mut cw = (ow * layout.size.clamp(0.1, 0.5)).round().max(8.0);
    let aspect = cam.width() as f32 / cam.height().max(1) as f32;
    let mut ch = if layout.shape == CameraShape::Circle { cw } else { (cw / aspect.max(0.1)).round() };
    if ch > oh - margin * 2.0 {
        ch = (oh - margin * 2.0).max(8.0);
        cw = if layout.shape == CameraShape::Circle { ch } else { (ch * aspect).round() };
    }
    // A circle shows the middle of the picture, square.
    let source = if layout.shape == CameraShape::Circle {
        let side = cam.width().min(cam.height());
        cam.crop((cam.width() - side) / 2, (cam.height() - side) / 2, side, side)
    } else {
        Some(cam.clone())
    };
    let Some(small) = source.and_then(|s| scale(&s, cw as u32, ch as u32)) else { return };
    let x0 = match layout.corner {
        Corner::TopLeft | Corner::BottomLeft => margin,
        Corner::TopRight | Corner::BottomRight => ow - margin - cw,
    }
    .max(0.0) as usize;
    let y0 = match layout.corner {
        Corner::TopLeft | Corner::TopRight => margin,
        Corner::BottomLeft | Corner::BottomRight => oh - margin - ch,
    }
    .max(0.0) as usize;
    let (sw, ow_px) = (small.width() as usize, out.width() as usize);
    let dst = out.pixels_mut();
    for (y, row) in small.pixels().chunks_exact(sw * 4).enumerate() {
        for (x, p) in row.as_chunks::<4>().0.iter().enumerate() {
            let a = coverage(layout.shape, x as f32, y as f32, cw, ch);
            if a <= 0.0 {
                continue;
            }
            let i = ((y0 + y) * ow_px + x0 + x) * 4;
            if let Some(d) = dst.get_mut(i..i + 3) {
                for (dv, sv) in d.iter_mut().zip(p) {
                    *dv = (f32::from(*sv) * a + f32::from(*dv) * (1.0 - a)).round() as u8;
                }
            }
        }
    }
}

/// The edited sound: `count` stereo samples from timeline sample `first`, every track that
/// isn't muted mixed at its volume. Interleaved, -1 to 1.
pub fn mix(rec: &Recording, timeline: &Timeline, first: u64, count: usize) -> Vec<f32> {
    let mut out = vec![0f32; count * 2];
    let end = first.saturating_add(count as u64);
    for t in &timeline.tracks {
        if t.muted || t.kind.is_video() || t.volume <= 0.0 {
            continue;
        }
        let stereo = rec.stream(t.stream).is_some_and(|s| s.channels >= 2);
        for c in &t.clips {
            let (at, from, len) = (to_samples(c.at), to_samples(c.from), to_samples(c.len));
            let (a, b) = (first.max(at), end.min(at.saturating_add(len)));
            if a >= b {
                continue;
            }
            let n = (b - a) as usize;
            let src = rec.sound(t.stream, from + (a - at), n);
            let gain = t.volume / 32768.0;
            let base = (a - first) as usize * 2;
            for k in 0..n {
                let (l, r) = if stereo {
                    (src.get(k * 2).copied().unwrap_or(0), src.get(k * 2 + 1).copied().unwrap_or(0))
                } else {
                    let v = src.get(k).copied().unwrap_or(0);
                    (v, v)
                };
                if let Some(o) = out.get_mut(base + k * 2..base + k * 2 + 2) {
                    o[0] += f32::from(l) * gain;
                    o[1] += f32::from(r) * gain;
                }
            }
        }
    }
    for s in &mut out {
        *s = s.clamp(-1.0, 1.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::Color;

    use super::*;
    use crate::recording::{Header, RecordingWriter, StreamInfo};

    fn rec() -> Arc<Recording> {
        let streams = vec![
            StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 64, height: 48, channels: 0 },
            StreamInfo { id: 1, kind: StreamKind::Microphone, name: String::new(), width: 0, height: 0, channels: 1 },
            StreamInfo { id: 2, kind: StreamKind::SystemSound, name: String::new(), width: 0, height: 0, channels: 2 },
        ];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        for t in 0..20u64 {
            let shade = (t * 10) as u8;
            w.video_frame(0, t * 100_000, &Raster::filled(64, 48, Color::rgb(shade, 0, 0)).unwrap()).unwrap();
            w.audio(1, t * 4800, &vec![1000; 4800]).unwrap();
            w.audio(2, t * 4800, &[2000i16, -2000].repeat(4800)).unwrap();
        }
        Arc::new(Recording::open(Box::new(w.finish(2_000_000).unwrap())).unwrap())
    }

    #[test]
    fn the_picture_follows_the_edits() {
        let r = rec();
        let mut tl = Timeline::new(&r);
        tl.cut(500_000, 1_500_000).unwrap();
        let mut c = Composer::new(r.clone(), &tl, 64, 48);
        // At 0.6 s the edited video shows what was recorded at 1.6 s.
        assert_eq!(c.frame(&tl, 600_000).pixel(3, 3).unwrap().r, 160);
        assert_eq!(c.frame(&tl, 100_000).pixel(3, 3).unwrap().r, 10);
        // Past the end, black.
        assert_eq!(c.frame(&tl, 5_000_000).pixel(3, 3).unwrap(), Color::BLACK);
    }

    #[test]
    fn sound_is_mixed_at_each_tracks_volume_and_muted_tracks_are_silent() {
        let r = rec();
        let mut tl = Timeline::new(&r);
        let s = mix(&r, &tl, 100, 4);
        let (m, sl, sr) = (1000.0 / 32768.0, 2000.0 / 32768.0, -2000.0 / 32768.0);
        assert!((s[0] - (m + sl)).abs() < 1e-6 && (s[1] - (m + sr)).abs() < 1e-6);
        tl.set_muted(2, true).unwrap();
        tl.set_volume(1, 0.5).unwrap();
        let s = mix(&r, &tl, 100, 1);
        assert!((s[0] - m * 0.5).abs() < 1e-6 && (s[1] - m * 0.5).abs() < 1e-6);
        // Where a clip was taken out there is silence.
        tl.split(1_000_000, Some(1)).unwrap();
        tl.delete_clip(1, 1).unwrap();
        assert!(mix(&r, &tl, to_samples(1_500_000), 10).iter().all(|v| *v == 0.0));
    }

    #[test]
    fn scaling_down_averages_and_sizes_fit_and_stay_even() {
        let mut r = Raster::filled(4, 2, Color::WHITE).unwrap();
        for i in [0usize, 4, 16, 20] {
            r.pixels_mut()[i..i + 3].copy_from_slice(&[0, 0, 0]);
        }
        let s = scale(&r, 2, 1).unwrap();
        assert_eq!(s.pixel(0, 0).unwrap().r, 0);
        assert_eq!(s.pixel(1, 0).unwrap().r, 255);
        assert_eq!(fit(3840, 2160, 1920, 1080), (1920, 1080));
        assert_eq!(fit(1201, 961, 1920, 1080), (1200, 960));
        assert_eq!(fit(5000, 100, 1920, 1080), (1920, 38));
    }

    #[test]
    fn without_the_screen_the_camera_is_the_picture() {
        // Recorded with the camera only: its pictures fill the video, as large as fits.
        let streams = vec![StreamInfo { id: 3, kind: StreamKind::Camera, name: "Cam".into(), width: 64, height: 48, channels: 0 }];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        let jpeg =
            screengrabcraft_render::encode(&Raster::filled(32, 18, Color::WHITE).unwrap(), screengrabcraft_model::ImageFormat::Jpeg, 90).unwrap();
        for t in 0..10u64 {
            w.jpeg_frame(3, t * 100_000, &jpeg).unwrap();
        }
        let r = Arc::new(Recording::open(Box::new(w.finish(1_000_000).unwrap())).unwrap());
        let tl = Timeline::new(&r);
        let mut c = Composer::new(r.clone(), &tl, 64, 48);
        let f = c.frame(&tl, 300_000);
        // 32 × 18 made 64 × 36, in the middle: black above and below, white across.
        assert!(f.pixel(0, 24).unwrap().r > 200 && f.pixel(63, 24).unwrap().r > 200);
        assert_eq!(f.pixel(32, 2).unwrap().r, 0);
        assert_eq!(f.pixel(32, 46).unwrap().r, 0);
        // And on its own: a picture of any shape, into a picture of any size.
        for (cw, ch, ow, oh) in [(1, 1, 7, 3), (640, 480, 2, 2), (3, 900, 1280, 720), (16, 9, 1, 1)] {
            let mut out = Raster::filled(ow, oh, Color::BLACK).unwrap();
            fill_camera(&mut out, &Raster::filled(cw, ch, Color::WHITE).unwrap());
            assert!(out.pixels().as_chunks::<4>().0.iter().any(|p| p[0] == 255), "{cw}x{ch} into {ow}x{oh}");
        }
    }

    #[test]
    fn captions_are_drawn_at_the_bottom_in_each_speakers_colour() {
        let r = rec();
        let tl = Timeline::new(&r);
        let mut c = Composer::new(r.clone(), &tl, 320, 180);
        let cues = vec![
            Cue { start: 0, end: 1_000_000, speaker: Speaker::You, text: "Hello there".into() },
            Cue { start: 500_000, end: 1_500_000, speaker: Speaker::Others, text: "Hi".into() },
        ];
        c.set_captions(Some(Arc::new(cues)));
        let bright = |f: &Raster, rows: std::ops::Range<u32>, yellow: bool| {
            rows.flat_map(|y| (0..f.width()).map(move |x| (x, y)))
                .filter_map(|(x, y)| f.pixel(x, y))
                .any(|p| p.r > 200 && p.g > 200 && if yellow { p.b < 150 } else { p.b > 200 })
        };
        // At 0.2 s only your line: yellow, in the bottom third, nothing at the top.
        let f = c.frame(&tl, 200_000);
        assert!(bright(&f, 120..180, true));
        assert!(!bright(&f, 0..100, true) && !bright(&f, 0..180, false));
        // At 0.7 s both: the others' white one too.
        let f = c.frame(&tl, 700_000);
        assert!(bright(&f, 90..180, false) && bright(&f, 90..180, true));
        // Past both, none.
        let f = c.frame(&tl, 1_600_000);
        assert!(!bright(&f, 0..180, true) && !bright(&f, 0..180, false));
        // At a video's size, for the eye (SGC_SHOTS=<folder>).
        if let Ok(dir) = std::env::var("SGC_SHOTS") {
            let mut pic = Raster::filled(1280, 720, Color::rgb(0xE8, 0xEC, 0xF2)).unwrap();
            let you = Cue { start: 0, end: 1, speaker: Speaker::You, text: "Here's the new login page: the button moved to the top right.".into() };
            let them = Cue { start: 0, end: 1, speaker: Speaker::Others, text: "It looks good to me.".into() };
            draw_captions(&mut pic, &[&them, &you]);
            let png = screengrabcraft_render::encode(&pic, screengrabcraft_model::ImageFormat::Png, 90).unwrap();
            std::fs::write(format!("{dir}/burned-captions.png"), png).unwrap();
        }
        // Any size of picture, any text: never a panic.
        for (w, h) in [(1, 1), (16, 16), (17, 3000), (3000, 17)] {
            let mut pic = Raster::filled(w, h, Color::BLACK).unwrap();
            let long = Cue { start: 0, end: 1, speaker: Speaker::Others, text: "word ".repeat(80) };
            draw_captions(&mut pic, &[&long, &long, &long]);
        }
    }

    #[test]
    fn a_camera_that_starts_late_shows_from_its_first_picture_on() {
        // The screen from the start, the camera only from 6 s (it was slow to start).
        let streams = vec![
            StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 100, height: 60, channels: 0 },
            StreamInfo { id: 3, kind: StreamKind::Camera, name: "Cam".into(), width: 32, height: 24, channels: 0 },
        ];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        w.video_frame(0, 0, &Raster::filled(100, 60, Color::BLACK).unwrap()).unwrap();
        let jpeg =
            screengrabcraft_render::encode(&Raster::filled(32, 24, Color::WHITE).unwrap(), screengrabcraft_model::ImageFormat::Jpeg, 90).unwrap();
        for t in 60..100u64 {
            w.jpeg_frame(3, t * 100_000, &jpeg).unwrap();
        }
        let r = Arc::new(Recording::open(Box::new(w.finish(10_000_000).unwrap())).unwrap());
        let tl = Timeline::new(&r);
        let mut c = Composer::new(r.clone(), &tl, 100, 60);
        let white = |f: &Raster| f.pixels().as_chunks::<4>().0.iter().filter(|p| p[0] > 200).count();
        assert_eq!(white(&c.frame(&tl, 3_000_000)), 0, "no camera before its first picture");
        assert!(white(&c.frame(&tl, 8_000_000)) > 100, "the camera in its corner after");
        assert!(white(&c.frame(&tl, 9_950_000)) > 100, "to the end");
    }

    #[test]
    fn hidden_tracks_are_left_out_and_a_camera_that_stopped_isnt_frozen() {
        // A white screen, and a grey camera that sends pictures for the first second only.
        let streams = vec![
            StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 100, height: 60, channels: 0 },
            StreamInfo { id: 3, kind: StreamKind::Camera, name: "Cam".into(), width: 32, height: 24, channels: 0 },
        ];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        w.video_frame(0, 0, &Raster::filled(100, 60, Color::WHITE).unwrap()).unwrap();
        let grey = Color::rgb(128, 128, 128);
        let jpeg = screengrabcraft_render::encode(&Raster::filled(32, 24, grey).unwrap(), screengrabcraft_model::ImageFormat::Jpeg, 90).unwrap();
        for t in 0..10u64 {
            w.jpeg_frame(3, t * 100_000, &jpeg).unwrap();
        }
        let r = Arc::new(Recording::open(Box::new(w.finish(10_000_000).unwrap())).unwrap());
        let mut tl = Timeline::new(&r);
        let (screen, camera) = (tl.find(StreamKind::Screen).unwrap(), tl.find(StreamKind::Camera).unwrap());
        let count = |f: &Raster, pick: &dyn Fn(&[u8; 4]) -> bool| f.pixels().as_chunks::<4>().0.iter().filter(|p| pick(p)).count();
        let is_grey = |p: &[u8; 4]| (100..160).contains(&p[0]);
        let is_white = |p: &[u8; 4]| p[0] > 230;
        let mut c = Composer::new(r.clone(), &tl, 100, 60);
        let f = c.frame(&tl, 500_000);
        assert!(count(&f, &is_grey) > 100 && count(&f, &is_white) > 1000, "both shown");
        // The camera hidden: the screen alone.
        tl.set_muted(camera, true).unwrap();
        let f = c.frame(&tl, 500_000);
        assert_eq!(count(&f, &is_grey), 0);
        // The screen hidden instead: the camera is the picture.
        tl.set_muted(camera, false).unwrap();
        tl.set_muted(screen, true).unwrap();
        let f = c.frame(&tl, 500_000);
        assert_eq!(count(&f, &is_white), 0);
        assert!(count(&f, &is_grey) > 1000);
        // Long after the camera's last picture: no frozen camera.
        tl.set_muted(screen, false).unwrap();
        let f = c.frame(&tl, 6_000_000);
        assert_eq!(count(&f, &is_grey), 0);
    }

    #[test]
    fn the_camera_lands_in_its_corner_and_a_circle_leaves_the_corners_alone() {
        let r = rec();
        let mut tl = Timeline::new(&r);
        tl.camera.corner = Corner::TopLeft;
        tl.camera.shape = CameraShape::Circle;
        tl.camera.size = 0.5;
        let mut out = Raster::filled(100, 100, Color::BLACK).unwrap();
        overlay_camera(&mut out, &Raster::filled(40, 30, Color::WHITE).unwrap(), &tl);
        // Margin 3 (2.5 %), a 50 px circle: its middle is white, its box's corner isn't.
        assert_eq!(out.pixel(28, 28).unwrap().r, 255);
        assert_eq!(out.pixel(4, 4).unwrap().r, 0);
        assert_eq!(out.pixel(80, 80).unwrap().r, 0);
    }
}
