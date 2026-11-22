//! What the edited video shows and plays at a moment: the screen's picture with the camera's
//! on top in a corner and the captions at the bottom ([`Composer`]), and the sound tracks mixed
//! ([`mix`]).

use std::sync::Arc;

use screengrabcraft_model::Raster;

use crate::captions::{self, Cue, Speaker};
use crate::recording::{FrameEntry, Recording, ScreenDecoder, StreamKind};
use crate::timeline::{Blend, CameraShape, Corner, Timeline};

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
    /// The screen's other clip while a transition turns one into the other (decoded on its own,
    /// so the one playing goes on forward).
    screen_other: Option<ScreenDecoder>,
    camera: Option<(usize, u8)>,
    /// The last camera picture decoded, and which it was (and the other clip's, in a transition).
    camera_frame: Option<(usize, Raster)>,
    camera_other: Option<(usize, Raster)>,
    /// Captions drawn on the picture (in timeline time), if any.
    captions: Option<Arc<Vec<Cue>>>,
    /// The media bin's pictures, for the layers' items.
    media: Arc<dyn crate::layer_draw::MediaStore>,
    /// The arrow drawn for a pointer the recording has no picture of.
    arrow: Option<crate::pointer::Shape>,
    texts: crate::layer_draw::TextCache,
    pictures: crate::layer_draw::PictureCache,
    pub width: u32,
    pub height: u32,
    /// Whether the zooms are shown (they are left out while one is being placed, so the whole
    /// screen shows with its frame).
    pub zoom: bool,
}

impl Composer {
    /// `width × height` is the video's size (the screen's, or smaller).
    pub fn new(rec: Arc<Recording>, timeline: &Timeline, width: u32, height: u32) -> Composer {
        let screen = timeline.find(StreamKind::Screen).and_then(|i| timeline.tracks.get(i).map(|t| (i, ScreenDecoder::new(t.stream))));
        let camera = timeline.find(StreamKind::Camera).and_then(|i| timeline.tracks.get(i).map(|t| (i, t.stream)));
        let screen_other = screen.as_ref().map(|(_, d)| ScreenDecoder::new(d.stream()));
        Composer {
            rec,
            screen,
            screen_other,
            camera,
            camera_frame: None,
            camera_other: None,
            captions: None,
            media: Arc::new(crate::layer_draw::NoMedia),
            arrow: crate::pointer::arrow(),
            texts: Default::default(),
            pictures: Default::default(),
            width: width.max(2),
            height: height.max(2),
            zoom: true,
        }
    }

    /// The media bin's pictures, for the layers' media items (without them, only their words and
    /// transitions are drawn).
    pub fn set_media(&mut self, media: Arc<dyn crate::layer_draw::MediaStore>) {
        self.media = media;
    }

    /// The picture at timeline time `t` µs. A hidden track (`muted`) isn't in it: without the
    /// screen the camera is the picture.
    pub fn frame(&mut self, timeline: &Timeline, t: u64) -> Raster {
        let (w, h) = (self.width, self.height);
        let black =
            || Raster::filled(w, h, screengrabcraft_model::Color::BLACK).unwrap_or_else(|_| Raster::single(screengrabcraft_model::Color::BLACK));
        let shown = |i: usize| timeline.tracks.get(i).is_some_and(|tr| !tr.muted);
        let screen_shown = self.screen.as_ref().is_some_and(|(i, _)| shown(*i));
        // The part of the screen shown (a zoom), unless zooms are left out (one being placed).
        let view = if self.zoom { crate::zoom::view_at(timeline, t) } else { crate::zoom::View::WHOLE };
        let fit = |pic: Raster| {
            if !view.is_whole() {
                zoomed(&pic, view, w, h)
            } else if (pic.width(), pic.height()) == (w, h) {
                Some(pic)
            } else {
                scale(&pic, w, h)
            }
        };
        let mut out = match &mut self.screen {
            Some((i, dec)) if screen_shown => {
                let pic = timeline.source_time(*i, t).and_then(|src| dec.frame_at(&self.rec, src)).and_then(fit).unwrap_or_else(black);
                // In a transition between two of its clips, the other one's picture with it: the
                // one going before the cut, the one coming after it.
                match (timeline.blend_at(*i, t), &mut self.screen_other) {
                    (Some(b), Some(other)) => {
                        let coming_plays = t >= b.cut;
                        let src = if coming_plays { b.going } else { b.coming };
                        match other.frame_at(&self.rec, src).and_then(fit) {
                            Some(o) if coming_plays => blend_pictures(&o, &pic, b.kind, b.p),
                            Some(o) => blend_pictures(&pic, &o, b.kind, b.p),
                            None => pic,
                        }
                    }
                    _ => pic,
                }
            }
            _ => black(),
        };
        // The mouse pointer on the screen, as it was then (larger, smoothed, its clicks ringed),
        // zoomed with it.
        if screen_shown
            && let Some((i, dec)) = &self.screen
            && let Some(src) = timeline.source_time(*i, t)
            && let Some(s) = self.rec.stream(dec.stream())
        {
            let (left, top, vw, _) = view.pixels(s.width, s.height);
            let f = w as f32 / vw.max(1.0);
            crate::pointer::draw_view(&mut out, &self.rec.pointer, timeline.pointer, src, (f, left, top), self.arrow.as_ref());
        }
        if let Some((i, stream)) = self.camera
            && shown(i)
            && let Some(src) = timeline.source_time(i, t)
            && let Some(cam) = self.camera_at(stream, src, false)
        {
            // A transition between two of its clips: the other one's picture blended in.
            let cam = match timeline.blend_at(i, t) {
                Some(b) => {
                    let coming_plays = t >= b.cut;
                    match self.camera_at(stream, if coming_plays { b.going } else { b.coming }, true) {
                        Some(o) if coming_plays => blend_pictures(&o, &cam, b.kind, b.p),
                        Some(o) => blend_pictures(&cam, &o, b.kind, b.p),
                        None => cam,
                    }
                }
                None => cam,
            };
            let plate = self.rec.plate();
            if screen_shown {
                overlay_camera_keyed(&mut out, &cam, timeline, plate.as_deref());
            } else {
                // Recorded (or shown) without the screen: the camera is the picture (what's
                // taken away from it is black).
                fill_camera_keyed(&mut out, &cam, &timeline.camera, plate.as_deref());
            }
        }
        // The layers over it (media, titles, overlays, transitions), the shortcuts pressed, the
        // captions over them.
        crate::layer_draw::draw_layers(&mut out, timeline, self.media.as_ref(), t, &mut self.texts, &mut self.pictures);
        if timeline.keys.show
            && let Some((keys, alpha)) = crate::keys::showing(&self.rec.keys, timeline, t)
        {
            crate::keys::draw(&mut out, keys, timeline.keys, alpha);
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
    fn camera_at(&mut self, stream: u8, src: u64, other: bool) -> Option<Raster> {
        let frames: &[FrameEntry] = self.rec.frames(stream);
        let i = frames.partition_point(|f| f.pts <= src).checked_sub(1)?;
        if frames.get(i).is_some_and(|f| src.saturating_sub(f.pts) > CAMERA_STALE_US) {
            return None;
        }
        let slot = if other { &mut self.camera_other } else { &mut self.camera_frame };
        if let Some((at, pic)) = slot
            && *at == i
        {
            return Some(pic.clone());
        }
        let jpeg = self.rec.jpeg(frames.get(i)?).ok()?;
        let pic = screengrabcraft_render::decode(&jpeg).ok()?;
        let slot = if other { &mut self.camera_other } else { &mut self.camera_frame };
        *slot = Some((i, pic.clone()));
        Some(pic)
    }
}

/// The picture `p` of the way (0 to 1) through a transition from `going` to `coming` (made the
/// size of `going` if it isn't).
pub fn blend_pictures(going: &Raster, coming: &Raster, kind: Blend, p: f32) -> Raster {
    let (w, h) = (going.width(), going.height());
    let resized;
    let coming = if (coming.width(), coming.height()) == (w, h) {
        coming
    } else {
        match scale(coming, w, h) {
            Some(c) => {
                resized = c;
                &resized
            }
            None => return going.clone(),
        }
    };
    let p = if p.is_finite() { p.clamp(0.0, 1.0) } else { 1.0 };
    // Slow at its ends.
    let e = p * p * (3.0 - 2.0 * p);
    let (a, b) = (going.pixels(), coming.pixels());
    let (wf, hf) = (w as f32, h as f32);
    // How much of the coming picture a pixel shows: 0 to 1, soft over a few pixels at an edge.
    let soft = (wf.max(hf) * 0.012).max(1.0);
    let edge = |d: f32| (d / soft + 0.5).clamp(0.0, 1.0);
    let (cx, cy, reach) = (wf / 2.0, hf / 2.0, (wf * wf + hf * hf).sqrt() / 2.0 + soft);
    let mut out = Vec::with_capacity(a.len());
    let pixel = |px: &[u8], x: i64, y: i64| -> [u8; 4] {
        let (x, y) = (x.clamp(0, i64::from(w) - 1) as usize, y.clamp(0, i64::from(h) - 1) as usize);
        let i = (y * w as usize + x) * 4;
        match px.get(i..i + 4) {
            Some(&[r, g, bl, al]) => [r, g, bl, al],
            _ => [0, 0, 0, 255],
        }
    };
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let (xi, yi) = (i64::from(x), i64::from(y));
            let mix = |k: f32| -> [u8; 4] {
                let (s, t) = (pixel(a, xi, yi), pixel(b, xi, yi));
                std::array::from_fn(|c| (f32::from(s[c]) * (1.0 - k) + f32::from(t[c]) * k).round() as u8)
            };
            let px = match kind {
                Blend::CrossFade => mix(e),
                Blend::DipBlack => {
                    let (s, k) = if p < 0.5 { (pixel(a, xi, yi), 1.0 - 2.0 * p) } else { (pixel(b, xi, yi), 2.0 * p - 1.0) };
                    [(f32::from(s[0]) * k) as u8, (f32::from(s[1]) * k) as u8, (f32::from(s[2]) * k) as u8, s[3]]
                }
                Blend::WipeLeft => mix(edge(fx - wf * (1.0 - e))),
                Blend::WipeRight => mix(edge(wf * e - fx)),
                Blend::WipeUp => mix(edge(fy - hf * (1.0 - e))),
                Blend::WipeDown => mix(edge(hf * e - fy)),
                Blend::Push => {
                    let shift = (wf * e).round() as i64;
                    let split = i64::from(w) - shift;
                    if xi < split { pixel(a, xi + shift, yi) } else { pixel(b, xi - split, yi) }
                }
                Blend::Circle => {
                    let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                    mix(edge(reach * e - d))
                }
            };
            out.extend_from_slice(&px);
        }
    }
    Raster::new(w, h, out).unwrap_or_else(|_| going.clone())
}

/// The size a video of this picture is made at: no larger than `max_w × max_h`, sides even.
pub fn fit(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let f = (f64::from(max_w) / f64::from(width.max(1))).min(f64::from(max_h) / f64::from(height.max(1))).min(1.0);
    let even = |v: f64| (((v.round() as u32).max(16)) / 2) * 2;
    (even(f64::from(width) * f), even(f64::from(height) * f))
}

/// The part `view` of `src`, made `w × h`: averaged where it's made smaller, smoothly blended
/// between pixels where it's made larger (a close zoom isn't blocky).
pub fn zoomed(src: &Raster, view: crate::zoom::View, w: u32, h: u32) -> Option<Raster> {
    let (sw, sh) = (src.width(), src.height());
    if sw == 0 || sh == 0 || w == 0 || h == 0 {
        return None;
    }
    let (left, top, vw, vh) = view.pixels(sw, sh);
    if vw >= w as f32 && vh >= h as f32 {
        // Made smaller: cut out the whole pixels of the view, then averaged down.
        let (x, y) = (left.floor() as u32, top.floor() as u32);
        let cw = (vw.round() as u32).clamp(1, sw.saturating_sub(x).max(1));
        let ch = (vh.round() as u32).clamp(1, sh.saturating_sub(y).max(1));
        let part = src.crop(x, y, cw, ch)?;
        return scale(&part, w, h);
    }
    let px = src.pixels();
    let (fx, fy) = (vw / w as f32, vh / h as f32);
    let at = |x: usize, y: usize| -> [f32; 4] {
        let i = (y * sw as usize + x) * 4;
        match px.get(i..i + 4) {
            Some(&[r, g, b, a]) => [f32::from(r), f32::from(g), f32::from(b), f32::from(a)],
            _ => [0.0; 4],
        }
    };
    let (mx, my) = (sw as usize - 1, sh as usize - 1);
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for oy in 0..h {
        let sy = (top + (oy as f32 + 0.5) * fy - 0.5).clamp(0.0, my as f32);
        let (y0, ty) = (sy.floor() as usize, sy - sy.floor());
        let y1 = (y0 + 1).min(my);
        for ox in 0..w {
            let sx = (left + (ox as f32 + 0.5) * fx - 0.5).clamp(0.0, mx as f32);
            let (x0, tx) = (sx.floor() as usize, sx - sx.floor());
            let x1 = (x0 + 1).min(mx);
            let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
            for k in 0..4 {
                let top_row = a[k] + (b[k] - a[k]) * tx;
                let bottom_row = c[k] + (d[k] - c[k]) * tx;
                out.push((top_row + (bottom_row - top_row) * ty).round().clamp(0.0, 255.0) as u8);
            }
        }
    }
    Raster::new(w, h, out).ok()
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
        Speaker::Others | Speaker::Media => screengrabcraft_model::Color::WHITE,
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
    fill_camera_keyed(out, cam, &crate::timeline::CameraLayout::default(), None);
}

/// How much of each pixel of the camera's picture (as drawn, `pic`) is kept: what the layout
/// takes away (a colour, or the background learnt in `plate`) gone; none when nothing is taken
/// away (or the background hasn't been learnt yet). A colour's glow is taken out of `pic`.
pub fn camera_keep(pic: &mut Raster, layout: &crate::timeline::CameraLayout, plate: Option<&crate::keyer::Plate>) -> Option<Vec<u8>> {
    use crate::keyer::{Remove, colour_key, plate_key};
    match layout.remove {
        Remove::Nothing => None,
        Remove::Colour => Some(colour_key(pic, layout.key_colour, layout.key_tolerance)),
        Remove::Background => plate.map(|p| plate_key(pic, p)),
    }
}

/// [`fill_camera`], with what the layout takes away from the camera's picture taken away (black
/// there).
pub fn fill_camera_keyed(out: &mut Raster, cam: &Raster, layout: &crate::timeline::CameraLayout, plate: Option<&crate::keyer::Plate>) {
    let (ow, oh) = (out.width(), out.height());
    let f = (f64::from(ow) / f64::from(cam.width().max(1))).min(f64::from(oh) / f64::from(cam.height().max(1)));
    let cw = ((f64::from(cam.width()) * f).round() as u32).clamp(1, ow.max(1));
    let ch = ((f64::from(cam.height()) * f).round() as u32).clamp(1, oh.max(1));
    let Some(mut small) = (if (cw, ch) == (cam.width(), cam.height()) { Some(cam.clone()) } else { scale(cam, cw, ch) }) else { return };
    let keep = camera_keep(&mut small, layout, plate);
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
            if let Some(keep) = &keep {
                for (x, p) in d.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let a = f32::from(keep.get(y * sw + x).copied().unwrap_or(255)) / 255.0;
                    for c in p.iter_mut().take(3) {
                        *c = (f32::from(*c) * a).round() as u8;
                    }
                }
            }
        }
    }
}

/// Where the camera's picture goes on a `ow × oh` video, for a camera picture of `aspect`
/// (width over height): x, y, width, height in pixels. In its corner, or where it was dragged
/// (kept whole on the picture).
pub fn camera_rect(layout: &crate::timeline::CameraLayout, aspect: f32, ow: f32, oh: f32) -> (f32, f32, f32, f32) {
    let margin = (ow * 0.025).round();
    let mut cw = (ow * layout.size.clamp(0.1, 0.5)).round().max(8.0);
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 4.0 / 3.0 };
    let mut ch = if layout.shape == CameraShape::Circle { cw } else { (cw / aspect.max(0.1)).round() };
    if ch > oh - margin * 2.0 {
        ch = (oh - margin * 2.0).max(8.0);
        cw = if layout.shape == CameraShape::Circle { ch } else { (ch * aspect).round() };
    }
    cw = cw.min(ow).max(1.0);
    ch = ch.min(oh).max(1.0);
    let (x, y) = match layout.at {
        Some([x, y]) => ((x * ow - cw / 2.0).round(), (y * oh - ch / 2.0).round()),
        None => (
            match layout.corner {
                Corner::TopLeft | Corner::BottomLeft => margin,
                Corner::TopRight | Corner::BottomRight => ow - margin - cw,
            },
            match layout.corner {
                Corner::TopLeft | Corner::TopRight => margin,
                Corner::BottomLeft | Corner::BottomRight => oh - margin - ch,
            },
        ),
    };
    // In a corner it stays on the picture; put somewhere by hand it may be part off it, or all.
    if layout.at.is_some() {
        return (x, y, cw, ch);
    }
    (x.clamp(0.0, (ow - cw).max(0.0)), y.clamp(0.0, (oh - ch).max(0.0)), cw, ch)
}

/// Draws the camera's picture in its place on `out`, in its shape.
pub fn overlay_camera(out: &mut Raster, cam: &Raster, timeline: &Timeline) {
    overlay_camera_keyed(out, cam, timeline, None);
}

/// [`overlay_camera`], with what the layout takes away from the camera's picture (a colour, or
/// the background learnt in `plate`) showing the screen there.
pub fn overlay_camera_keyed(out: &mut Raster, cam: &Raster, timeline: &Timeline, plate: Option<&crate::keyer::Plate>) {
    let layout = timeline.camera;
    let (ow, oh) = (out.width() as f32, out.height() as f32);
    let aspect = cam.width() as f32 / cam.height().max(1) as f32;
    let (x0, y0, cw, ch) = camera_rect(&layout, aspect, ow, oh);
    // A circle shows the middle of the picture, square.
    let source = if layout.shape == CameraShape::Circle {
        let side = cam.width().min(cam.height());
        cam.crop((cam.width() - side) / 2, (cam.height() - side) / 2, side, side)
    } else {
        Some(cam.clone())
    };
    let Some(mut small) = source.and_then(|s| scale(&s, cw as u32, ch as u32)) else { return };
    let keep = camera_keep(&mut small, &layout, plate);
    // (It may be part off the picture: only what is on it is drawn.)
    let (x0, y0) = (x0 as i64, y0 as i64);
    let (sw, ow_px, oh_px) = (small.width() as usize, out.width() as i64, out.height() as i64);
    let dst = out.pixels_mut();
    for (y, row) in small.pixels().chunks_exact(sw * 4).enumerate() {
        for (x, p) in row.as_chunks::<4>().0.iter().enumerate() {
            let kept = keep.as_ref().map_or(1.0, |k| f32::from(k.get(y * sw + x).copied().unwrap_or(255)) / 255.0);
            let a = coverage(layout.shape, x as f32, y as f32, cw, ch) * kept;
            if a <= 0.0 {
                continue;
            }
            let (dx, dy) = (x0 + x as i64, y0 + y as i64);
            if dx < 0 || dy < 0 || dx >= ow_px || dy >= oh_px {
                continue;
            }
            let i = ((dy * ow_px + dx) * 4) as usize;
            if let Some(d) = dst.get_mut(i..i + 3) {
                for (dv, sv) in d.iter_mut().zip(p) {
                    *dv = (f32::from(*sv) * a + f32::from(*dv) * (1.0 - a)).round() as u8;
                }
            }
        }
    }
}

/// The edited sound: `count` stereo samples from timeline sample `first`, every track that
/// isn't muted mixed at its volume. Interleaved, -1 to 1. (The layers' media have no sound
/// here: see [`mix_with`].)
pub fn mix(rec: &Recording, timeline: &Timeline, first: u64, count: usize) -> Vec<f32> {
    mix_with(rec, timeline, &crate::layer_draw::NoMedia, first, count)
}

/// [`mix`], with the sound of the layers' media from `media`.
pub fn mix_with(rec: &Recording, timeline: &Timeline, media: &dyn crate::layer_draw::MediaStore, first: u64, count: usize) -> Vec<f32> {
    let mut out = vec![0f32; count * 2];
    crate::layer_draw::mix_layers(&mut out, timeline, media, first, count);
    let end = first.saturating_add(count as u64);
    for (ti, t) in timeline.tracks.iter().enumerate() {
        if t.muted || t.kind.is_video() || t.volume <= 0.0 {
            continue;
        }
        let stereo = rec.stream(t.stream).is_some_and(|s| s.channels >= 2);
        // `n` stereo samples of the track as recorded from sample `from` (-1 to 1).
        let read = |from: u64, n: usize| -> Vec<[f32; 2]> {
            let src = rec.sound(t.stream, from, n);
            (0..n)
                .map(|k| {
                    let (l, r) = if stereo {
                        (src.get(k * 2).copied().unwrap_or(0), src.get(k * 2 + 1).copied().unwrap_or(0))
                    } else {
                        let v = src.get(k).copied().unwrap_or(0);
                        (v, v)
                    };
                    [f32::from(l) / 32768.0, f32::from(r) / 32768.0]
                })
                .collect()
        };
        // The same from any sample (before the recording's start: silence).
        let raw = |start: i64, n: usize| -> Vec<[f32; 2]> {
            let skip = usize::try_from(start.saturating_neg().max(0)).unwrap_or(n).min(n);
            let mut s = vec![[0.0; 2]; skip];
            s.extend(read(u64::try_from(start.max(0)).unwrap_or(0), n - skip));
            s
        };
        // With its background noise taken out, if it's to be.
        let noise = (t.denoise > 0.0).then(|| rec.noise(t.stream));
        let source = |start: i64, n: usize| -> Vec<[f32; 2]> {
            match &noise {
                Some(p) => crate::denoise::cleaned(&raw, p, t.denoise, stereo, start, n),
                None => raw(start, n),
            }
        };
        for (ci, c) in t.clips.iter().enumerate() {
            // Cross-faded with the clip before and the one after where they meet: it plays on
            // into them for half of each cross-fade, coming up as the other goes down.
            let lead = timeline.transition_span(ti, ci).map_or(0, |(s, _, _)| c.at - s);
            let tail = timeline.transition_span(ti, ci + 1).map_or(0, |(_, e, _)| e - c.end());
            let (at, len) = (to_samples(c.at.saturating_sub(lead)), to_samples(c.len + lead + tail));
            let (a, b) = (first.max(at), end.min(at.saturating_add(len)));
            if a >= b {
                continue;
            }
            let n = (b - a) as usize;
            // Where it starts in the recording, the cross-fade's lead in (before the recording's
            // start: silence).
            let lead_src = c.source_at(lead).saturating_sub(c.from);
            let from = i64::try_from(to_samples(c.from)).unwrap_or(i64::MAX / 4) - i64::try_from(to_samples(lead_src)).unwrap_or(0);
            let sound = if c.speed == crate::timeline::NORMAL_SPEED {
                source(from.saturating_add_unsigned(a - at), n)
            } else {
                // Faster or slower, at its own pitch.
                crate::stretch::stretched(&|start, n| source(from.saturating_add(start), n), c.rate(), a - at, n)
            };
            let base = (a - first) as usize * 2;
            let faded = c.fade_in > 0 || c.fade_out > 0 || lead > 0 || tail > 0;
            let (cs, ce) = (c.at, c.end());
            for (k, [l, r]) in sound.iter().enumerate() {
                let gain = if faded {
                    let u = to_us(a + k as u64);
                    let x = if u < cs.saturating_add(lead) && lead > 0 {
                        cross_rise((u + lead).saturating_sub(cs) as f32 / (2 * lead) as f32)
                    } else if u >= ce.saturating_sub(tail) && tail > 0 {
                        cross_rise((ce + tail).saturating_sub(u) as f32 / (2 * tail) as f32)
                    } else {
                        1.0
                    };
                    t.volume * c.fade_at(u.saturating_sub(cs).min(c.len)) * x
                } else {
                    t.volume
                };
                if let Some(o) = out.get_mut(base + k * 2..base + k * 2 + 2) {
                    o[0] += l * gain;
                    o[1] += r * gain;
                }
            }
        }
    }
    mix_clicks(&mut out, rec, timeline, first, count);
    for s in &mut out {
        *s = s.clamp(-1.0, 1.0);
    }
    out
}

/// How loud a clip is `x` of the way (0 to 1) into a cross-fade it comes up in: the two together
/// keep the same loudness (a quarter of a sine wave).
fn cross_rise(x: f32) -> f32 {
    (x.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2).sin()
}

/// The pointer's clicks heard (when its look says so), where the screen's clips play them: each
/// click of the recording that a clip plays is heard where it is on the timeline.
fn mix_clicks(out: &mut [f32], rec: &Recording, timeline: &Timeline, first: u64, count: usize) {
    let look = timeline.pointer;
    if !look.show || !look.click_sound || rec.pointer.clicks().is_empty() {
        return;
    }
    let Some(track) = timeline.find(StreamKind::Screen).and_then(|i| timeline.tracks.get(i)).filter(|t| !t.muted) else { return };
    let clicks = rec.pointer.clicks();
    let end = first.saturating_add(count as u64);
    let len = crate::pointer::CLICK_SOUND_LEN as u64;
    for c in &track.clips {
        // The clicks this clip plays, if it plays near the samples asked for.
        let (at, clip_end) = (to_samples(c.at), to_samples(c.end()));
        if clip_end.saturating_add(len) <= first || at >= end {
            continue;
        }
        let (src_a, src_b) = (c.from, c.from.saturating_add(c.source_len()));
        let from = clicks.partition_point(|k| k.t < src_a);
        for k in clicks.iter().skip(from).take_while(|k| k.t < src_b).take(10_000) {
            let start = to_samples(c.at.saturating_add(c.played(k.t - src_a)));
            if start >= end || start.saturating_add(len) <= first {
                continue;
            }
            for s in start.max(first)..start.saturating_add(len).min(end) {
                let v = crate::pointer::click_sound((s - start) as usize);
                let i = (s - first) as usize * 2;
                if let Some(o) = out.get_mut(i..i + 2) {
                    o[0] += v;
                    o[1] += v;
                }
            }
        }
    }
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

    /// A white screen (320 × 240, 3 s) with the pointer moving right along y = 100, a green
    /// square for its picture (its hot spot at its corner), a click at 2 s.
    fn rec_with_pointer() -> Arc<Recording> {
        use crate::pointer::{LEFT, SHOWING, Sample, Shape};
        let streams = vec![StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 320, height: 240, channels: 0 }];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        w.pointer_shape(&Shape { id: 1, hot: (0, 0), nominal: 16, pic: Raster::filled(16, 16, Color::rgb(0, 200, 0)).unwrap() }).unwrap();
        for t in 0..30u64 {
            w.video_frame(0, t * 100_000, &Raster::filled(320, 240, Color::WHITE).unwrap()).unwrap();
            let flags = SHOWING | if t == 20 { LEFT } else { 0 };
            w.pointer(&Sample { t: t * 100_000, x: 20.0 + t as f32 * 8.0, y: 100.0, flags, shape: 1 }).unwrap();
        }
        Arc::new(Recording::open(Box::new(w.finish(3_000_000).unwrap())).unwrap())
    }

    #[test]
    fn the_recorded_pointer_is_on_the_video_where_it_was_and_follows_the_edits() {
        let r = rec_with_pointer();
        assert_eq!((r.pointer.samples().len(), r.pointer.shapes().len(), r.pointer.clicks().len()), (30, 1, 1));
        let mut tl = Timeline::new(&r);
        tl.pointer.smooth = 0.0;
        tl.pointer.clicks = false;
        let green = |pic: &Raster| -> Vec<(usize, usize)> {
            let w = pic.width() as usize;
            pic.pixels().chunks(4).enumerate().filter(|(_, p)| p[1] > 150 && p[0] < 80).map(|(i, _)| (i % w, i / w)).collect()
        };
        let mut c = Composer::new(r.clone(), &tl, 320, 240);
        // At 1 s it was at x = 100: its square from there.
        let at = green(&c.frame(&tl, 1_000_000));
        assert_eq!(at.len(), 16 * 16);
        assert_eq!((at[0], at[at.len() - 1]), ((100, 100), (115, 115)));
        // Twice as large: four times the pixels, from the same corner (where it points).
        tl.pointer.size = 2.0;
        let big = green(&c.frame(&tl, 1_000_000));
        assert!(big.len() > 900 && big[0].0 <= 101 && big[0].1 <= 101, "{} {:?}", big.len(), big.first());
        tl.pointer.size = 1.0;
        // Hidden: not on the video.
        tl.pointer.show = false;
        assert!(green(&c.frame(&tl, 1_000_000)).is_empty());
        tl.pointer.show = true;
        // The first second cut: at 0.5 s the video shows 1.5 s of the recording, pointer and all.
        tl.cut(0, 1_000_000).unwrap();
        let mut c = Composer::new(r.clone(), &tl, 320, 240);
        assert_eq!(green(&c.frame(&tl, 500_000)).first(), Some(&(140, 100)));
        // A video made half the size: it's half the size, at half the place.
        let mut half = Composer::new(r.clone(), &tl, 160, 120);
        let small = green(&half.frame(&tl, 500_000));
        assert_eq!(small.first(), Some(&(70, 50)));
        assert!(small.len() >= 49 && small.len() <= 81, "{}", small.len());
        // A recording without a pointer is as it was.
        let plain = rec();
        let tl = Timeline::new(&plain);
        assert!(plain.pointer.is_empty());
        let _ = Composer::new(plain, &tl, 64, 48).frame(&tl, 0);
    }

    #[test]
    fn the_pointers_clicks_are_heard_where_the_edits_put_them() {
        let r = rec_with_pointer();
        let mut tl = Timeline::new(&r);
        let loud = |tl: &Timeline, us: u64| mix(&r, tl, to_samples(us), 960).iter().fold(0f32, |m, v| m.max(v.abs()));
        assert_eq!(loud(&tl, 2_000_000), 0.0, "not unless asked");
        tl.pointer.click_sound = true;
        assert!(loud(&tl, 2_000_000) > 0.1);
        assert_eq!(loud(&tl, 1_000_000), 0.0);
        // The first second cut: heard at 1 s; then twice as fast: at half a second.
        tl.cut(0, 1_000_000).unwrap();
        assert!(loud(&tl, 1_000_000) > 0.1 && loud(&tl, 2_000_000) == 0.0);
        tl.set_speed(0, 2_000_000, 200, None).unwrap();
        assert!(loud(&tl, 500_000) > 0.1);
        // The pointer or the screen hidden: none.
        tl.pointer.show = false;
        assert_eq!(loud(&tl, 500_000), 0.0);
        tl.pointer.show = true;
        tl.set_muted(0, true).unwrap();
        assert_eq!(loud(&tl, 500_000), 0.0);
    }

    /// A screen 64 × 48 with its right half blue, the rest red, for 2 s.
    fn rec_halves() -> Arc<Recording> {
        let streams = vec![StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 64, height: 48, channels: 0 }];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        let px: Vec<u8> = (0..48).flat_map(|_| (0..64).flat_map(|x| if x >= 32 { [0, 0, 255, 255] } else { [255, 0, 0, 255] })).collect();
        let pic = Raster::new(64, 48, px).unwrap();
        for t in 0..20u64 {
            w.video_frame(0, t * 100_000, &pic).unwrap();
        }
        Arc::new(Recording::open(Box::new(w.finish(2_000_000).unwrap())).unwrap())
    }

    #[test]
    fn a_zoom_fills_the_video_with_its_frame_and_lets_go_after() {
        use crate::layers::{Anim, AnimKind, Item, Placement, What};
        let r = rec_halves();
        let mut tl = Timeline::new(&r);
        // Into the right half's middle, at once, from 0.5 s to 1.5 s.
        let cut = Anim { kind: AnimKind::None, len: 0 };
        let zoom = Item {
            at: 500_000,
            len: 1_000_000,
            what: What::Zoom,
            place: Placement { x: 0.75, y: 0.5, w: 0.4, opacity: 1.0 },
            intro: cut,
            outro: cut,
            ..Item::default()
        };
        tl.place(zoom, None, 500_000).unwrap();
        let mut c = Composer::new(r.clone(), &tl, 64, 48);
        let blue = Color::rgb(0, 0, 255);
        let zoomed = c.frame(&tl, 1_000_000);
        assert_eq!((zoomed.pixel(1, 1).unwrap(), zoomed.pixel(62, 46).unwrap()), (blue, blue), "all of it is the blue half");
        let whole = c.frame(&tl, 1_700_000);
        assert_eq!((whole.pixel(1, 1).unwrap(), whole.pixel(62, 1).unwrap()), (Color::rgb(255, 0, 0), blue), "the whole screen again");
        // Left out while one is being placed.
        c.zoom = false;
        assert_eq!(c.frame(&tl, 1_000_000).pixel(1, 1).unwrap(), Color::rgb(255, 0, 0));
        // A larger video of a zoom is smooth, not blocky: the edge between the halves is blended.
        let big = zoomed_edge();
        assert!(big.0 > 0 && big.1 > 0, "{big:?}");
    }

    /// The red and blue of the pixel at the halves' edge when a 4 × 2 picture is zoomed 8 times.
    fn zoomed_edge() -> (u8, u8) {
        let pic = Raster::new(4, 2, [[255u8, 0, 0, 255]; 2].into_iter().chain([[0, 0, 255, 255]; 2]).flatten().cycle().take(32).collect()).unwrap();
        let out = zoomed(&pic, crate::zoom::View { x: 0.25, y: 0.0, w: 0.5, h: 1.0 }, 16, 2).unwrap();
        let p = out.pixel(8, 0).unwrap();
        (p.r, p.b)
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
    fn a_transition_between_two_clips_blends_them_across_the_cut() {
        use crate::timeline::Blend;
        let r = rec();
        let mut tl = Timeline::new(&r);
        // 0.5 s to 1.5 s cut out: at 0.5 s the picture recorded at 0.5 s turns into that at 1.5 s.
        tl.cut(500_000, 1_500_000).unwrap();
        assert!(tl.set_transition(0, 0, Some(Blend::CrossFade), None).is_err(), "the first clip has none before it");
        tl.set_transition(0, 1, Some(Blend::CrossFade), Some(400_000)).unwrap();
        assert_eq!(tl.transition_span(0, 1).map(|(a, b, _)| (a, b)), Some((300_000, 700_000)));
        let mut c = Composer::new(r.clone(), &tl, 64, 48);
        let red = |c: &mut Composer, tl: &Timeline, t: u64| c.frame(tl, t).pixel(3, 3).unwrap().r;
        // Before and after it, each clip as it is.
        assert_eq!(red(&mut c, &tl, 200_000), 20);
        assert_eq!(red(&mut c, &tl, 800_000), 180);
        // At the cut, halfway: between the one going (0.5 s: 50) and the one coming (1.5 s: 150).
        let mid = red(&mut c, &tl, 500_000);
        assert!((90..=110).contains(&mid), "{mid}");
        // Played back from before it, each moment is the same picture as asked for on its own.
        let mut fresh = Composer::new(r.clone(), &tl, 64, 48);
        assert_eq!(red(&mut fresh, &tl, 600_000), red(&mut c, &tl, 600_000));
        // A wipe to the right: the coming clip on the left, the going one still on the right.
        tl.set_transition(0, 1, Some(Blend::WipeRight), None).unwrap();
        let f = c.frame(&tl, 500_000);
        assert_eq!((f.pixel(2, 20).unwrap().r, f.pixel(61, 20).unwrap().r), (150, 50));
        // Moved apart, they don't meet: a plain cut again.
        tl.move_clip(0, 1, 600_000).unwrap();
        assert_eq!(tl.transition_span(0, 1), None);
        assert_eq!(red(&mut c, &tl, 450_000), 40);
    }

    #[test]
    fn sound_cross_fades_where_two_clips_meet() {
        use crate::timeline::Blend;
        let r = rec();
        let mut tl = Timeline::new(&r);
        tl.set_muted(2, true).unwrap();
        tl.split(1_000_000, Some(1)).unwrap();
        tl.set_transition(1, 1, Some(Blend::Push), Some(400_000)).unwrap();
        assert_eq!(tl.tracks[1].clips[1].transition.map(|x| x.kind), Some(Blend::CrossFade), "sound only cross-fades");
        // Across the cut the microphone's steady sound stays about as loud (the two together).
        let m = 1000.0 / 32768.0;
        for t in [850_000u64, 1_000_000, 1_150_000] {
            let s = mix(&r, &tl, to_samples(t), 1);
            assert!(s[0] > m * 0.95 && s[0] < m * 1.45, "{t}: {}", s[0]);
        }
        // Away from it, as it was.
        assert!((mix(&r, &tl, to_samples(500_000), 1)[0] - m).abs() < 1e-6);
    }

    #[test]
    fn every_blend_goes_from_one_picture_to_the_other() {
        use crate::timeline::Blend;
        let a = Raster::filled(40, 30, Color::rgb(200, 0, 0)).unwrap();
        let b = Raster::filled(40, 30, Color::rgb(0, 0, 200)).unwrap();
        for kind in Blend::ALL {
            assert_eq!(blend_pictures(&a, &b, kind, 0.0).pixels(), a.pixels(), "{kind:?} starts on the one going");
            assert_eq!(blend_pictures(&a, &b, kind, 1.0).pixels(), b.pixels(), "{kind:?} ends on the one coming");
            // Of another size, or junk: no panic.
            let small = Raster::filled(7, 3, Color::WHITE).unwrap();
            assert_eq!(blend_pictures(&a, &small, kind, f32::NAN).width(), 40);
        }
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
    fn a_sound_clip_fades_in_and_out_and_a_steady_hum_is_taken_out() {
        let r = rec();
        let mut tl = Timeline::new(&r);
        tl.set_muted(2, true).unwrap();
        let m = 1000.0 / 32768.0;
        let at = |tl: &Timeline, us: u64| mix(&r, tl, to_samples(us), 1)[0];
        // A second up from silence, half a second down to it, on the 2 s microphone.
        tl.set_fade(1, 0, Some(1_000_000), Some(500_000)).unwrap();
        assert!(at(&tl, 0).abs() < 1e-6);
        assert!((at(&tl, 500_000) - m * 0.5).abs() < m * 0.02, "half way up: {}", at(&tl, 500_000));
        assert!((at(&tl, 1_200_000) - m).abs() < 1e-6, "all the way up");
        assert!((at(&tl, 1_750_000) - m * 0.5).abs() < m * 0.02, "half way down");
        // Split in the middle: the first piece keeps the fade in, the second the fade out.
        tl.split(1_000_000, Some(1)).unwrap();
        let c = &tl.tracks[1].clips;
        assert_eq!((c[0].fade_in, c[0].fade_out, c[1].fade_in, c[1].fade_out), (1_000_000, 0, 0, 500_000));
        assert!((at(&tl, 1_200_000) - m).abs() < 1e-6);
        // The microphone's steady hum (all it has) is learnt as its noise and taken out.
        let mut tl = Timeline::new(&r);
        tl.set_muted(2, true).unwrap();
        tl.set_denoise(1, 1.0).unwrap();
        let after = at(&tl, 1_000_000);
        assert!(after.abs() < m * 0.3, "{after} of {m}");
        assert!(tl.set_denoise(0, 1.0).is_err(), "the screen has no noise to take out");
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
    fn the_cameras_green_screen_or_learnt_background_shows_the_screen_through() {
        use crate::keyer::{GREEN, Remove};
        // A white screen; a camera of a green screen with a red box (the person) in its middle.
        let streams = vec![
            StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 200, height: 100, channels: 0 },
            StreamInfo { id: 3, kind: StreamKind::Camera, name: "Cam".into(), width: 80, height: 60, channels: 0 },
        ];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        w.video_frame(0, 0, &Raster::filled(200, 100, Color::WHITE).unwrap()).unwrap();
        let mut cam = Raster::filled(80, 60, Color::rgb(GREEN[0], GREEN[1], GREEN[2])).unwrap();
        for y in 10..60u32 {
            for x in 30..50u32 {
                let i = ((y * 80 + x) * 4) as usize;
                cam.pixels_mut()[i..i + 3].copy_from_slice(&[0xE0, 0x20, 0x20]);
            }
        }
        let jpeg = screengrabcraft_render::encode(&cam, screengrabcraft_model::ImageFormat::Jpeg, 95).unwrap();
        for t in 0..10u64 {
            w.jpeg_frame(3, t * 100_000, &jpeg).unwrap();
        }
        let r = Arc::new(Recording::open(Box::new(w.finish(1_000_000).unwrap())).unwrap());
        let mut tl = Timeline::new(&r);
        tl.camera.shape = CameraShape::Rectangle;
        tl.camera.size = 0.5;
        let mut c = Composer::new(r.clone(), &tl, 200, 100);
        let count = |f: &Raster, pick: &dyn Fn(&[u8; 4]) -> bool| f.pixels().as_chunks::<4>().0.iter().filter(|p| pick(p)).count();
        let green = |p: &[u8; 4]| p[1] > 120 && p[0] < 80;
        let red = |p: &[u8; 4]| p[0] > 180 && p[1] < 80;
        let f = c.frame(&tl, 500_000);
        assert!(count(&f, &green) > 1000, "the green screen as recorded");
        // Its colour taken away: the screen shows there, the person stays.
        tl.camera.remove = Remove::Colour;
        let f = c.frame(&tl, 500_000);
        assert!(count(&f, &green) < 20, "{}", count(&f, &green));
        assert!(count(&f, &red) > 300);
        // The background learnt instead (no plate yet: the camera as it is, until it's learnt).
        tl.camera.remove = Remove::Background;
        assert!(count(&c.frame(&tl, 500_000), &green) > 1000);
        let mut matte = vec![0u8; 80 * 60];
        for y in 10..60usize {
            for x in 30..50usize {
                matte[y * 80 + x] = 255;
            }
        }
        let learnt = crate::keyer::learn(&[(
            screengrabcraft_render::decode(&jpeg).unwrap(),
            screengrabcraft_render::matte::Matte::new(80, 60, matte).unwrap(),
        )]);
        r.set_plate(learnt.unwrap());
        let f = c.frame(&tl, 500_000);
        assert!(count(&f, &green) < 60, "{}", count(&f, &green));
        assert!(count(&f, &red) > 300);
        // Without the screen the camera is the picture: what's taken away is black.
        tl.camera.remove = Remove::Colour;
        tl.set_muted(0, true).unwrap();
        let f = c.frame(&tl, 500_000);
        assert!(count(&f, &green) < 20 && count(&f, &red) > 300);
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
