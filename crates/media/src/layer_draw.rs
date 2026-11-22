//! Drawing the layers over the recording's picture and mixing their sound in: media (pictures,
//! other recordings), titles, lower thirds, callouts and overlays drawn in code, and the
//! transitions' effect on the picture under them. Each item comes in and goes out as its
//! animations say.

use std::collections::HashMap;
use std::sync::Arc;

use screengrabcraft_model::{Color, Pt, Raster, Rect, Style, TextAlign};
use screengrabcraft_render::{canvas, draw, text};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

use crate::layers::{self, AnimKind, Item, LayerKind, MediaKind, TextStyle, TransitionKind, What};
use crate::timeline::Timeline;

/// The pictures and sound of the media bin, as the host has read them.
pub trait MediaStore: Send + Sync {
    /// Its picture `t` µs into it (a picture has one; a recording, the one at that moment); none
    /// while it's being read, or if it can't be.
    fn picture(&self, id: u32, t: u64) -> Option<Arc<Raster>>;
    /// `count` stereo samples at 48 kHz from sample `first` of it, interleaved, -1 to 1.
    fn sound(&self, id: u32, first: u64, count: usize) -> Vec<f32>;
}

/// No media: the layers' words and transitions are still drawn.
pub struct NoMedia;

impl MediaStore for NoMedia {
    fn picture(&self, _id: u32, _t: u64) -> Option<Arc<Raster>> {
        None
    }
    fn sound(&self, _id: u32, _first: u64, count: usize) -> Vec<f32> {
        vec![0.0; count * 2]
    }
}

/// Text items drawn already (a frame-sized see-through picture each), by what they are.
#[derive(Default)]
pub struct TextCache {
    drawn: HashMap<String, Arc<Pixmap>>,
    /// The broadcast graphics built already, by what they are.
    built: HashMap<String, Arc<crate::stock::Graphic>>,
}

impl TextCache {
    fn get(&mut self, key: String, make: impl FnOnce() -> Option<Pixmap>) -> Option<Arc<Pixmap>> {
        if let Some(p) = self.drawn.get(&key) {
            return Some(p.clone());
        }
        let p = Arc::new(make()?);
        // A few dozen at most: an edited title is drawn again under its new words.
        if self.drawn.len() > 48 {
            self.drawn.clear();
        }
        self.drawn.insert(key, p.clone());
        Some(p)
    }
}

/// A broadcast graphic for an item (built once for its words, colour and length).
fn graphic(
    cache: Option<&mut TextCache>,
    style: TextStyle,
    title: &str,
    subtitle: &str,
    color: &str,
    len: u64,
    size: f32,
) -> Option<Arc<crate::stock::Graphic>> {
    let key = format!("{style:?}|{title}|{subtitle}|{color}|{len}|{size}");
    if let Some(g) = cache.as_ref().and_then(|c| c.built.get(&key)) {
        return Some(g.clone());
    }
    let g = Arc::new(crate::stock::build(style, title, subtitle, layers::color(color, [0x0b, 0x5f, 0xff]), len, size)?);
    if let Some(c) = cache {
        if c.built.len() > 48 {
            c.built.clear();
        }
        c.built.insert(key, g.clone());
    }
    Some(g)
}

/// Where a broadcast graphic (designed at 1920 × 1080, taking up `bounds` there) goes on a
/// `w × h` video: scaled to its width, kept as near the top or the bottom as it was designed,
/// moved by the item's place (the middle is where it was designed) and grown about its own middle.
/// A stinger covers the whole picture, whatever its shape.
fn stock_place(style: TextStyle, g: &crate::stock::Graphic, place: layers::Placement, w: f32, h: f32) -> Transform {
    let bounds = g.bounds;
    use crate::stock::{H, W};
    if crate::stock::is_stinger(style) {
        return Transform::from_scale(w / W, h / H);
    }
    let (cx, cy) = (bounds.x + bounds.w / 2.0, bounds.y + bounds.h / 2.0);
    let (k, grow) = (w / W, if g.sized { 1.0 } else { place.w.clamp(0.2, 3.0) });
    // An arrow grows about its tip, so it stays on what it points at.
    let (px, py) = if style == TextStyle::Arrow { g.anchor } else { (cx, cy) };
    Transform::from_translate(-px, -py)
        .post_scale(grow, grow)
        .post_translate(px + place.x * W - g.anchor.0, py + place.y * H - g.anchor.1)
        .post_scale(k, k)
        .post_translate(0.0, (h - H * k) * (cy / H))
}

/// Blurs `pix` inside `rect` (in its pixels), `amount` of the way (0 to 1): two passes of a box
/// blur across and down, each about a 45th of the picture's width, so words there can't be read.
/// The box is kept to the picture (what's past its edges is its edge again); an empty one, or
/// one that isn't a number, does nothing.
pub(crate) fn blur_box(pix: &mut Pixmap, rect: Rect, amount: f32) {
    let amount = if amount.is_finite() { amount.clamp(0.0, 1.0) } else { 0.0 };
    if amount <= 0.0 || ![rect.x, rect.y, rect.w, rect.h].iter().all(|v| v.is_finite()) {
        return;
    }
    let (pw, ph) = (pix.width() as usize, pix.height() as usize);
    let edge = |v: f32, most: usize| if v <= 0.0 { 0 } else { (v.round() as usize).min(most) };
    let (x0, x1) = (edge(rect.x.min(rect.x + rect.w), pw), edge(rect.x.max(rect.x + rect.w), pw));
    let (y0, y1) = (edge(rect.y.min(rect.y + rect.h), ph), edge(rect.y.max(rect.y + rect.h), ph));
    let (bw, bh) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    let Some(n) = bw.checked_mul(bh).and_then(|n| n.checked_mul(4)) else { return };
    if n == 0 {
        return;
    }
    let radius = ((pw as f32 / 45.0).round() as usize).clamp(3, 400);
    let data = pix.data_mut();
    // The box's pixels (premultiplied, as the picture holds them).
    let mut was = Vec::with_capacity(n);
    for y in y0..y1 {
        let start = y.saturating_mul(pw).saturating_add(x0).saturating_mul(4);
        match data.get(start..start.saturating_add(bw * 4)) {
            Some(row) => was.extend_from_slice(row),
            None => return,
        }
    }
    let mut blurred = was.clone();
    let mut between = vec![0u8; n];
    for _ in 0..2 {
        box_pass(&blurred, &mut between, bw, bh, radius, true);
        box_pass(&between, &mut blurred, bw, bh, radius, false);
    }
    for (row, y) in (y0..y1).enumerate() {
        let start = y.saturating_mul(pw).saturating_add(x0).saturating_mul(4);
        let from = row.saturating_mul(bw * 4);
        let (Some(out), Some(old), Some(new)) =
            (data.get_mut(start..start.saturating_add(bw * 4)), was.get(from..from + bw * 4), blurred.get(from..from + bw * 4))
        else {
            return;
        };
        for ((o, a), b) in out.iter_mut().zip(old).zip(new) {
            *o = (f32::from(*a) + (f32::from(*b) - f32::from(*a)) * amount).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// One pass of a box blur over a `w × h` picture of RGBA pixels, across its rows or down its
/// columns, `r` pixels each way (the edge pixel standing in for what's past the edge).
fn box_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize, across: bool) {
    let (lines, len) = if across { (h, w) } else { (w, h) };
    if len == 0 {
        return;
    }
    let at = |line: usize, i: usize| if across { (line * w + i) * 4 } else { (i * w + line) * 4 };
    let span = (2 * r + 1) as u32;
    let last = len - 1;
    for line in 0..lines {
        let px = |i: isize| -> [u32; 4] {
            let k = at(line, i.clamp(0, last as isize) as usize);
            match src.get(k..k + 4) {
                Some(&[r, g, b, a]) => [r.into(), g.into(), b.into(), a.into()],
                _ => [0; 4],
            }
        };
        let (ri, mut sum) = (r as isize, [0u32; 4]);
        for i in -ri..=ri {
            for (s, v) in sum.iter_mut().zip(px(i)) {
                *s = s.saturating_add(v);
            }
        }
        for i in 0..len {
            let k = at(line, i);
            if let Some(out) = dst.get_mut(k..k + 4) {
                for (o, s) in out.iter_mut().zip(sum) {
                    *o = (s.saturating_add(span / 2) / span).min(255) as u8;
                }
            }
            let (add, take) = (px(i as isize + ri + 1), px(i as isize - ri));
            for ((s, a), t) in sum.iter_mut().zip(add).zip(take) {
                *s = s.saturating_add(a).saturating_sub(t);
            }
        }
    }
}

/// A box through a transform (one that doesn't turn it).
fn moved(r: Rect, t: Transform) -> Rect {
    Rect::new(r.x * t.sx + t.tx, r.y * t.sy + t.ty, r.w * t.sx, r.h * t.sy)
}

/// How far an animation has got, eased (slow at its ends).
fn ease(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    p * p * (3.0 - 2.0 * p)
}

/// How an item is drawn at a moment: see-through, moved, grown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub alpha: f32,
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
}

/// An item's motion at timeline time `t` in a `w × h` video.
pub fn motion(item: &Item, t: u64, w: f32, h: f32) -> Motion {
    let (inn, out) = item.anim_at(t);
    let mut m = Motion { alpha: item.place.opacity, dx: 0.0, dy: 0.0, scale: 1.0 };
    for (kind, p) in [(item.intro.kind, 1.0 - ease(inn)), (item.outro.kind, ease(out))] {
        // `p`: how far from being there (0 is there).
        match kind {
            AnimKind::None => {}
            AnimKind::Fade => m.alpha *= 1.0 - p,
            AnimKind::SlideLeft => {
                m.dx -= p * w * 0.35;
                m.alpha *= 1.0 - p * 0.6;
            }
            AnimKind::SlideRight => {
                m.dx += p * w * 0.35;
                m.alpha *= 1.0 - p * 0.6;
            }
            AnimKind::SlideUp => {
                m.dy += p * h * 0.2;
                m.alpha *= 1.0 - p * 0.6;
            }
            AnimKind::Pop => {
                m.scale *= 1.0 - p * 0.85;
                m.alpha *= 1.0 - p * 0.4;
            }
        }
    }
    m.alpha = m.alpha.clamp(0.0, 1.0);
    m
}

/// Where a media item's picture of `pw × ph` goes on a `w × h` video (before its motion and its
/// turn): what its crop leaves of it, as wide as its place says.
pub fn media_rect(item: &Item, pw: u32, ph: u32, w: f32, h: f32) -> Rect {
    let look = item.look.clone().checked();
    let [l, t, r, b] = look.crop;
    let (kw, kh) = (pw.max(1) as f32 * (1.0 - l - r).max(0.1), ph.max(1) as f32 * (1.0 - t - b).max(0.1));
    let bw = (item.place.w * w).max(2.0);
    let bh = bw * kh / kw;
    Rect::new(item.place.x * w - bw / 2.0, item.place.y * h - bh / 2.0, bw, bh)
}

/// Where an item is on a `w × h` video (before its motion): for the designer's handles. None for
/// a transition, or media not read yet.
pub fn item_rect(item: &Item, timeline: &Timeline, store: &dyn MediaStore, w: f32, h: f32) -> Option<Rect> {
    match &item.what {
        What::Media { id } => {
            let m = timeline.media.iter().find(|m| m.id == *id)?;
            let (pw, ph) = if m.width > 0 && m.height > 0 {
                (m.width, m.height)
            } else {
                let p = store.picture(*id, 0)?;
                (p.width(), p.height())
            };
            Some(media_rect(item, pw, ph, w, h))
        }
        What::Text { style, title, subtitle, color } if crate::stock::is_stock(*style) => {
            let g = graphic(None, *style, title, subtitle, color, item.len, item.place.w)?;
            Some(moved(g.bounds, stock_place(*style, &g, item.place, w, h)))
        }
        What::Text { style, title, subtitle, color } => Some(text_item(None, *style, title, subtitle, color, item.place, w, h)),
        What::Transition { .. } => None,
        // The frame it zooms into: the video's shape, kept on the picture.
        What::Zoom => {
            let v = crate::zoom::frame(item);
            Some(Rect::new(v.x * w, v.y * h, v.w * w, v.h * h))
        }
    }
}

/// The colour of `#rrggbb`.
fn rgb(text: &str, or: [u8; 3]) -> Color {
    let [r, g, b] = layers::color(text, or);
    Color::rgb(r, g, b)
}

fn paint(c: Color, alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(draw::skia_color(c, alpha));
    p.anti_alias = true;
    p
}

/// Words laid out at `size`, wrapped at `max`: their size.
fn lay(words: &str, size: f32, bold: bool, max: f32) -> (text::Layout, Style) {
    let style = Style { font_size: size.max(4.0), bold, text_color: Color::WHITE, align: TextAlign::Left, ..Style::default() };
    let mut l = text::layout(words, &style, Some(max.max(20.0)));
    l.lines.truncate(4);
    l.width = l.lines.iter().map(|x| x.width).fold(0.0, f32::max);
    l.height = l.line_height * l.lines.len() as f32;
    (l, style)
}

fn words_at(canvas: &mut Pixmap, l: &text::Layout, style: &Style, x: f32, y: f32, c: Color, alpha: f32) {
    if let Some(path) = text::text_path(l, style, Pt::new(x, y), l.width) {
        canvas.fill_path(&path, &paint(c, alpha), FillRule::Winding, Transform::identity(), None);
    }
}

fn box_at(canvas: &mut Pixmap, r: Rect, radius: f32, c: Color, alpha: f32) {
    if let Some(path) = draw::rounded_rect(r, radius) {
        canvas.fill_path(&path, &paint(c, alpha), FillRule::Winding, Transform::identity(), None);
    }
}

/// A text item (or an overlay with no words) drawn on `canvas` (a `w × h` see-through picture)
/// if given; where it is, either way.
#[allow(clippy::too_many_arguments)]
pub fn text_item(
    canvas: Option<&mut Pixmap>,
    style: TextStyle,
    title: &str,
    subtitle: &str,
    color: &str,
    place: layers::Placement,
    w: f32,
    h: f32,
) -> Rect {
    let s = (h / 1080.0) * place.w.clamp(0.2, 3.0);
    let (cx, cy) = (place.x * w, place.y * h);
    let main = rgb(color, [0x6e, 0x5a, 0xd6]);
    let dark = Color::rgb(0x16, 0x16, 0x1c);
    let mut canvas = canvas;
    let mut c = |f: &mut dyn FnMut(&mut Pixmap)| {
        if let Some(cv) = canvas.as_deref_mut() {
            f(cv);
        }
    };
    match style {
        TextStyle::Title | TextStyle::TitleBox => {
            let (l, st) = lay(title, 84.0 * s, true, w * 0.86);
            let pad = if style == TextStyle::TitleBox { 34.0 * s } else { 0.0 };
            let r = Rect::new(cx - l.width / 2.0 - pad, cy - l.height / 2.0 - pad * 0.6, l.width + pad * 2.0, l.height + pad * 1.2);
            c(&mut |cv| {
                if style == TextStyle::TitleBox {
                    box_at(cv, r, 18.0 * s, main, 0.92);
                } else {
                    words_at(cv, &l, &st, r.x + 4.0 * s, r.y + 5.0 * s, Color::BLACK, 0.45);
                }
                let ink = if style == TextStyle::Title { main } else { Color::WHITE };
                words_at(cv, &l, &st, r.x + pad, r.y + pad * 0.6, ink, 1.0);
            });
            r
        }
        TextStyle::Banner => {
            let (l, st) = lay(title, 48.0 * s, true, w * 0.9);
            let bh = l.height + 36.0 * s;
            let r = Rect::new(0.0, cy - bh / 2.0, w, bh);
            c(&mut |cv| {
                box_at(cv, r, 0.0, main, 0.95);
                words_at(cv, &l, &st, cx - l.width / 2.0, r.y + 18.0 * s, Color::WHITE, 1.0);
            });
            r
        }
        TextStyle::LowerBar | TextStyle::LowerLine | TextStyle::LowerTag => {
            let (lt, st) = lay(title, 52.0 * s, true, w * 0.6);
            let (ls, ss) = lay(subtitle, 32.0 * s, false, w * 0.6);
            let pad = 22.0 * s;
            let inner_w = lt.width.max(ls.width);
            let bw = inner_w + pad * 2.0;
            let bh = lt.height + ls.height + pad * 2.0 + 10.0 * s;
            let r = Rect::new(cx - bw / 2.0, cy - bh / 2.0, bw, bh);
            c(&mut |cv| match style {
                TextStyle::LowerBar => {
                    box_at(cv, r, 6.0 * s, dark, 0.85);
                    box_at(cv, Rect::new(r.x, r.y, 10.0 * s, r.h), 0.0, main, 1.0);
                    words_at(cv, &lt, &st, r.x + pad + 6.0 * s, r.y + pad, Color::WHITE, 1.0);
                    words_at(cv, &ls, &ss, r.x + pad + 6.0 * s, r.y + pad + lt.height + 10.0 * s, Color::rgb(0xd8, 0xd8, 0xe0), 1.0);
                }
                TextStyle::LowerLine => {
                    box_at(cv, r, 4.0 * s, Color::BLACK, 0.45);
                    words_at(cv, &lt, &st, r.x + pad, r.y + pad, Color::WHITE, 1.0);
                    box_at(cv, Rect::new(r.x + pad, r.y + pad + lt.height + 2.0 * s, inner_w, 5.0 * s), 2.0 * s, main, 1.0);
                    words_at(cv, &ls, &ss, r.x + pad, r.y + pad + lt.height + 12.0 * s, Color::WHITE, 0.95);
                }
                _ => {
                    let top = Rect::new(r.x, r.y, lt.width + pad * 2.0, lt.height + pad);
                    box_at(cv, top, 4.0 * s, main, 1.0);
                    words_at(cv, &lt, &st, top.x + pad, top.y + pad / 2.0, dark, 1.0);
                    let low = Rect::new(r.x, top.y + top.h + 6.0 * s, ls.width + pad * 2.0, ls.height + pad * 0.8);
                    box_at(cv, low, 4.0 * s, dark, 0.92);
                    words_at(cv, &ls, &ss, low.x + pad, low.y + pad * 0.4, Color::WHITE, 1.0);
                }
            });
            r
        }
        TextStyle::Callout | TextStyle::Note => {
            let (l, st) = lay(title, 40.0 * s, style == TextStyle::Callout, w * 0.4);
            let pad = 20.0 * s;
            let r = Rect::new(cx - l.width / 2.0 - pad, cy - l.height / 2.0 - pad, l.width + pad * 2.0, l.height + pad * 2.0);
            let fill = if style == TextStyle::Callout { Color::WHITE } else { rgb(color, [0xff, 0xd8, 0x4d]) };
            c(&mut |cv| {
                // A soft shadow, the box, and (a callout) its pointer.
                box_at(cv, Rect::new(r.x + 5.0 * s, r.y + 7.0 * s, r.w, r.h), 14.0 * s, Color::BLACK, 0.25);
                box_at(cv, r, 14.0 * s, fill, 0.98);
                if style == TextStyle::Callout {
                    let mut pb = PathBuilder::new();
                    pb.move_to(r.x + r.w * 0.18, r.y + r.h - 1.0);
                    pb.line_to(r.x + r.w * 0.06, r.y + r.h + 34.0 * s);
                    pb.line_to(r.x + r.w * 0.36, r.y + r.h - 1.0);
                    pb.close();
                    if let Some(p) = pb.finish() {
                        cv.fill_path(&p, &paint(fill, 0.98), FillRule::Winding, Transform::identity(), None);
                    }
                }
                words_at(cv, &l, &st, r.x + pad, r.y + pad, dark, 1.0);
            });
            r
        }
        TextStyle::Highlight => {
            let bw = (place.w * w).max(8.0);
            let bh = bw * 0.6;
            let r = Rect::new(cx - bw / 2.0, cy - bh / 2.0, bw, bh);
            c(&mut |cv| {
                if let Some(path) = draw::rounded_rect(r, 10.0 * (h / 1080.0)) {
                    let stroke = Stroke { width: (8.0 * h / 1080.0).max(2.0), ..Stroke::default() };
                    cv.stroke_path(&path, &paint(main, 1.0), &stroke, Transform::identity(), None);
                }
            });
            r
        }
        TextStyle::Spotlight => {
            let bw = (place.w * w).max(8.0);
            let bh = bw * 0.6;
            let r = Rect::new(cx - bw / 2.0, cy - bh / 2.0, bw, bh);
            c(&mut |cv| {
                // Everything dimmed but an oval.
                let mut pb = PathBuilder::new();
                if let Some(full) = tiny_skia::Rect::from_xywh(0.0, 0.0, w.max(1.0), h.max(1.0)) {
                    pb.push_rect(full);
                }
                if let Some(oval) = tiny_skia::Rect::from_xywh(r.x, r.y, r.w.max(1.0), r.h.max(1.0)) {
                    pb.push_oval(oval);
                }
                if let Some(p) = pb.finish() {
                    cv.fill_path(&p, &paint(Color::BLACK, 0.62), FillRule::EvenOdd, Transform::identity(), None);
                }
            });
            r
        }
        // The broadcast graphics are played, not drawn here (see `draw_layers`): where one is.
        _ => graphic(None, style, title, subtitle, color, 4_000_000, place.w)
            .map_or(Rect::new(0.0, 0.0, w, h), |g| moved(g.bounds, stock_place(style, &g, place, w, h))),
    }
}

/// A transition's effect on the picture at `p` (0 to 1 through it).
fn transition(out: &mut Pixmap, kind: TransitionKind, p: f32) {
    let (w, h) = (out.width() as f32, out.height() as f32);
    let p = p.clamp(0.0, 1.0);
    let full = tiny_skia::Rect::from_xywh(0.0, 0.0, w, h);
    let mut cover = |c: Color, a: f32| {
        if let Some(r) = full
            && a > 0.0
        {
            out.fill_rect(r, &paint(c, a.clamp(0.0, 1.0)), Transform::identity(), None);
        }
    };
    match kind {
        TransitionKind::FadeIn => cover(Color::BLACK, 1.0 - ease(p)),
        TransitionKind::FadeOut => cover(Color::BLACK, ease(p)),
        TransitionKind::DipBlack => cover(Color::BLACK, 1.0 - (2.0 * p - 1.0).abs()),
        TransitionKind::DipWhite => cover(Color::WHITE, 1.0 - (2.0 * p - 1.0).abs()),
        TransitionKind::Flash => cover(Color::WHITE, (1.0 - p).powi(2) * (p * 6.0).min(1.0)),
        TransitionKind::Wipe => {
            // A dark band, soft at its edges, sweeping from the left to the right.
            let band = w * 0.45;
            let x = -band + ease(p) * (w + band);
            for (k, a) in [(0.0, 1.0), (-0.12, 0.5), (0.12, 0.5)] {
                if let Some(r) = tiny_skia::Rect::from_xywh(x + k * band, 0.0, band, h) {
                    out.fill_rect(r, &paint(Color::rgb(0x0b, 0x0b, 0x10), a), Transform::identity(), None);
                }
            }
        }
    }
}

/// Draws a picture (straight alpha) onto `out` in `rect`, moved and grown as `m` says.
fn picture_at(out: &mut Pixmap, pic: &Pixmap, rect: Rect, m: Motion) {
    turned_at(out, pic, rect, m, 0.0, Pt::new(0.0, 0.0));
}

/// [`picture_at`], turned `degrees` clockwise about `pivot` (where it is before its motion).
fn turned_at(out: &mut Pixmap, pic: &Pixmap, rect: Rect, m: Motion, degrees: f32, pivot: Pt) {
    if m.alpha <= 0.0 || pic.width() == 0 || pic.height() == 0 {
        return;
    }
    let (sx, sy) = (rect.w / pic.width() as f32, rect.h / pic.height() as f32);
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let mut t = Transform::from_scale(sx, sy)
        .post_translate(rect.x, rect.y)
        .post_translate(-cx, -cy)
        .post_scale(m.scale.max(0.01), m.scale.max(0.01))
        .post_translate(cx + m.dx, cy + m.dy);
    if degrees.abs() > 1.0e-3 && degrees.is_finite() {
        t = t.post_rotate_at(degrees, pivot.x + m.dx, pivot.y + m.dy);
    }
    let paint = PixmapPaint { opacity: m.alpha, quality: tiny_skia::FilterQuality::Bilinear, ..PixmapPaint::default() };
    out.draw_pixmap(0, 0, pic.as_ref(), &paint, t, None);
}

/// A picture or a video item's picture as its look frames it: what its crop leaves, in `rect`
/// (before its motion), turned about its middle, its corners rounded, with a border round it, a
/// device round that and a soft shadow under it if it has them.
fn framed_at(out: &mut Pixmap, pic: &Pixmap, rect: Rect, m: Motion, look: &layers::ItemLook) {
    if m.alpha <= 0.0 || pic.width() == 0 || pic.height() == 0 || rect.w < 1.0 || rect.h < 1.0 {
        return;
    }
    let look = look.clone().checked();
    let [l, t, r, b] = look.crop;
    let (pw, ph) = (pic.width() as f32, pic.height() as f32);
    // The part of the picture kept, in its pixels.
    let (kx, ky) = (pw * l, ph * t);
    let (kw, kh) = ((pw * (1.0 - l - r)).max(1.0), (ph * (1.0 - t - b)).max(1.0));
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let (hw, hh) = (rect.w / 2.0, rect.h / 2.0);
    // Its own space, its middle at 0; grown or shrunk, turned, and moved where it goes.
    let k = m.scale.max(0.01);
    let place = Transform::from_scale(k, k).post_rotate(look.rotate).post_translate(cx + m.dx, cy + m.dy);
    let radius = look.radius.max(look.device.screen_radius()) * rect.w.min(rect.h);
    let inner = Rect::new(-hw, -hh, rect.w, rect.h);
    let Some(shape) = draw::rounded_rect(inner, radius) else { return };
    if look.shadow {
        // A soft shadow under it (under its device if it has one).
        let body = look.device.outer(inner);
        let r = if look.device == crate::frame::Device::None { radius } else { body.w.min(body.h) * 0.04 };
        crate::frame::shadow(out, body, r, place, m.alpha);
    }
    crate::frame::draw_device(out, look.device, inner, place, m.alpha);
    // (The picture is placed as the shape is: a shader isn't moved with the path.)
    let to_shape = Transform::from_translate(-kx, -ky).post_scale(rect.w / kw, rect.h / kh).post_translate(-hw, -hh).post_concat(place);
    let fill = Paint {
        shader: tiny_skia::Pattern::new(pic.as_ref(), tiny_skia::SpreadMode::Pad, tiny_skia::FilterQuality::Bilinear, m.alpha, to_shape),
        anti_alias: true,
        ..Paint::default()
    };
    out.fill_path(&shape, &fill, FillRule::Winding, place, None);
    if look.border > 0.0 {
        let [r, g, b] = layers::color(&look.border_colour, [0xff, 0xff, 0xff]);
        let stroke = Stroke { width: (look.border * rect.w).max(1.0), ..Stroke::default() };
        out.stroke_path(&shape, &paint(Color::rgb(r, g, b), m.alpha), &stroke, place, None);
    }
}

/// Pictures read from the media bin, made ready to draw (premultiplied), by media id and the
/// picture they were made from.
#[derive(Default)]
pub struct PictureCache {
    ready: HashMap<u32, (usize, Arc<Pixmap>)>,
}

impl PictureCache {
    fn get(&mut self, id: u32, pic: &Arc<Raster>) -> Option<Arc<Pixmap>> {
        let key = Arc::as_ptr(pic) as usize;
        if let Some((k, p)) = self.ready.get(&id)
            && *k == key
        {
            return Some(p.clone());
        }
        let p = Arc::new(canvas::to_pixmap(pic)?);
        self.ready.insert(id, (key, p.clone()));
        Some(p)
    }
}

/// Draws every picture layer's items at timeline time `t` over `out` (the recording's picture),
/// the later layers on top. An item with a mask or a blend mode is drawn on a see-through picture
/// of its own first, masked, then laid on the rest as its blend mode says.
pub fn draw_layers(out: &mut Raster, timeline: &Timeline, store: &dyn MediaStore, t: u64, texts: &mut TextCache, pictures: &mut PictureCache) {
    draw_layers_at(out, timeline, store, t, 0, texts, pictures);
}

/// [`draw_layers`], an item moving then (sliding, growing, moving by its keyframes) blurred along
/// its way over `blur` µs round `t`: drawn at each of the shutter's moments, the pictures averaged.
pub fn draw_layers_at(
    out: &mut Raster,
    timeline: &Timeline,
    store: &dyn MediaStore,
    t: u64,
    blur: u64,
    texts: &mut TextCache,
    pictures: &mut PictureCache,
) {
    let showing: Vec<&Item> = timeline
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Picture && !l.hidden)
        .filter_map(|l| l.items.iter().find(|i| i.at <= t && t < i.end()))
        // A zoom draws nothing of its own.
        .filter(|i| !matches!(i.what, What::Zoom))
        .collect();
    if showing.is_empty() {
        return;
    }
    let Some(mut pix) = canvas::to_pixmap(out) else { return };
    let (w, h) = (pix.width() as f32, pix.height() as f32);
    let shutter = crate::timeline::MotionBlur { on: blur > 0, amount: blur as f32 / 40_000.0 };
    for whole in showing {
        // Where its keyframes have it then.
        let placed = whole.placed_at(t);
        let item: &Item = &placed;
        // A blur and a transition change what's under them: they're never drawn apart.
        let changes_under = matches!(item.what, What::Transition { .. } | What::Text { style: TextStyle::Blur, .. });
        let moments: Vec<u64> = if blur > 0 && !changes_under {
            let m: Vec<u64> = shutter.moments(t).into_iter().map(|s| s.clamp(whole.at, whole.end().saturating_sub(1))).collect();
            if moves(whole, &m, w, h) { m } else { Vec::new() }
        } else {
            Vec::new()
        };
        let apart = (item.look.is_layered() || !moments.is_empty()) && !changes_under;
        if !apart {
            let _ = draw_item(&mut pix, item, timeline, store, t, texts, pictures);
            continue;
        }
        let drawn = if moments.is_empty() {
            Pixmap::new(pix.width(), pix.height())
                .and_then(|mut own| draw_item(&mut own, item, timeline, store, t, texts, pictures).map(|b| (own, b)))
        } else {
            blurred(whole, &moments, t, timeline, store, texts, pictures, pix.width(), pix.height())
        };
        let Some((mut own, (bx, to))) = drawn else { continue };
        let look = item.look.clone().checked();
        if let Some(mask) = mask_for(&look.mask, bx, to, own.width(), own.height()) {
            own.apply_mask(&mask);
        }
        let paint = PixmapPaint { blend_mode: blend_mode(look.blend), ..PixmapPaint::default() };
        pix.draw_pixmap(0, 0, own.as_ref(), &paint, Transform::identity(), None);
    }
    if let Some(r) = canvas::from_pixmap(&pix) {
        *out = r;
    }
}

/// Whether an item moves (its place, its size, how its animations move or grow it) by about a
/// pixel or more over `moments` on a `w × h` video.
fn moves(item: &Item, moments: &[u64], w: f32, h: f32) -> bool {
    let at = |s: u64| {
        let p = item.placed_at(s);
        let m = motion(&p, s, w, h);
        (p.place.x * w + m.dx, p.place.y * h + m.dy, p.place.w * w * m.scale)
    };
    let (Some(a), Some(b)) = (moments.first(), moments.last()) else { return false };
    let ((ax, ay, aw), (bx, by, bw)) = (at(*a), at(*b));
    (ax - bx).abs() > 0.5 || (ay - by).abs() > 0.5 || (aw - bw).abs() > 0.5
}

/// An item drawn at each of `moments` on see-through pictures of its own, averaged: the picture
/// and (for its mask) where its box is at `t`.
#[allow(clippy::too_many_arguments)]
fn blurred(
    item: &Item,
    moments: &[u64],
    t: u64,
    timeline: &Timeline,
    store: &dyn MediaStore,
    texts: &mut TextCache,
    pictures: &mut PictureCache,
    w: u32,
    h: u32,
) -> Option<(Pixmap, (Rect, Transform))> {
    let mut sum = vec![0u32; (w as usize).checked_mul(h as usize)?.checked_mul(4)?];
    let mut middle = None;
    let mut n = 0u32;
    let centre = moments.get(moments.len() / 2).copied().unwrap_or(t);
    for &s in moments {
        let mut own = Pixmap::new(w, h)?;
        let b = draw_item(&mut own, &item.placed_at(s), timeline, store, s, texts, pictures);
        if s == centre || middle.is_none() {
            middle = b.or(middle);
        }
        for (a, v) in sum.iter_mut().zip(own.data()) {
            *a = a.saturating_add(u32::from(*v));
        }
        n += 1;
    }
    let mut out = Pixmap::new(w, h)?;
    let n = n.max(1);
    for (o, s) in out.data_mut().iter_mut().zip(sum) {
        *o = ((s + n / 2) / n).min(255) as u8;
    }
    Some((out, middle?))
}

/// tiny-skia's blend mode for an item's.
fn blend_mode(b: layers::BlendMode) -> tiny_skia::BlendMode {
    match b {
        layers::BlendMode::Normal => tiny_skia::BlendMode::SourceOver,
        layers::BlendMode::Multiply => tiny_skia::BlendMode::Multiply,
        layers::BlendMode::Screen => tiny_skia::BlendMode::Screen,
        layers::BlendMode::Overlay => tiny_skia::BlendMode::Overlay,
        layers::BlendMode::Darken => tiny_skia::BlendMode::Darken,
        layers::BlendMode::Lighten => tiny_skia::BlendMode::Lighten,
    }
}

/// The outline of a mask's shape filling `r`.
fn mask_path(shape: layers::MaskShape, r: Rect) -> Option<tiny_skia::Path> {
    use layers::MaskShape as S;
    if !(r.w > 0.0 && r.h > 0.0 && r.x.is_finite() && r.y.is_finite() && r.w.is_finite() && r.h.is_finite()) {
        return None;
    }
    let at = |u: f32, v: f32| (r.x + u * r.w, r.y + v * r.h);
    let polygon = |points: &[(f32, f32)]| {
        let mut pb = PathBuilder::new();
        for (k, (u, v)) in points.iter().enumerate() {
            let (x, y) = at(*u, *v);
            if k == 0 {
                pb.move_to(x, y);
            } else {
                pb.line_to(x, y);
            }
        }
        pb.close();
        pb.finish()
    };
    match shape {
        S::None => None,
        S::Rectangle => Some(PathBuilder::from_rect(tiny_skia::Rect::from_xywh(r.x, r.y, r.w, r.h)?)),
        S::Rounded => draw::rounded_rect(r, r.w.min(r.h) * 0.2),
        S::Ellipse => PathBuilder::from_oval(tiny_skia::Rect::from_xywh(r.x, r.y, r.w, r.h)?),
        S::Diamond => polygon(&[(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)]),
        S::Star => {
            // Five points, the top one up; the points half its size from its middle, the dips a fifth.
            let points: Vec<(f32, f32)> = (0..10)
                .map(|k| {
                    let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
                    let reach = if k % 2 == 0 { 0.5 } else { 0.2 };
                    (0.5 + reach * a.cos(), 0.53 + reach * a.sin())
                })
                .collect();
            polygon(&points)
        }
        S::Heart => {
            // Two round lobes at the top meeting in a point at the bottom.
            let mut pb = PathBuilder::new();
            let (x, y) = at(0.5, 0.3);
            pb.move_to(x, y);
            for [a, b, c] in [
                [(0.5, 0.0), (0.0, 0.0), (0.0, 0.3)],
                [(0.0, 0.6), (0.5, 0.8), (0.5, 1.0)],
                [(0.5, 0.8), (1.0, 0.6), (1.0, 0.3)],
                [(1.0, 0.0), (0.5, 0.0), (0.5, 0.3)],
            ] {
                let ((ax, ay), (bx, by), (cx, cy)) = (at(a.0, a.1), at(b.0, b.1), at(c.0, c.1));
                pb.cubic_to(ax, ay, bx, by, cx, cy);
            }
            pb.close();
            pb.finish()
        }
    }
}

/// A `w × h` mask of an item's mask shape: `bx` is the item's box in its own space and `to` puts
/// that space on the picture. None when it has no mask.
fn mask_for(mask: &layers::Mask, bx: Rect, to: Transform, w: u32, h: u32) -> Option<tiny_skia::Mask> {
    let m = mask.checked();
    let (mw, mh) = (bx.w * m.w, bx.h * m.h);
    let path = mask_path(m.shape, Rect::new(bx.x + bx.w * m.x - mw / 2.0, bx.y + bx.h * m.y - mh / 2.0, mw, mh))?;
    let mut out = tiny_skia::Mask::new(w, h)?;
    out.fill_path(&path, FillRule::Winding, true, to);
    if m.cut {
        out.invert();
    }
    Some(out)
}

/// The transform that moves, grows and turns a frame-sized picture of an item as its motion and
/// its turn say (`pivot`: its middle before its motion).
fn frame_transform(m: Motion, w: f32, h: f32, degrees: f32, pivot: Pt) -> Transform {
    let (cx, cy, k) = (w / 2.0, h / 2.0, m.scale.max(0.01));
    let t = Transform::from_translate(-cx, -cy).post_scale(k, k).post_translate(cx + m.dx, cy + m.dy);
    if degrees.abs() > 1.0e-3 && degrees.is_finite() { t.post_rotate_at(degrees, pivot.x + m.dx, pivot.y + m.dy) } else { t }
}

/// Draws one item at timeline time `t` onto `pix` (the picture so far, or a see-through one of its
/// own): where its box is then, in its own space, and the transform putting that on the picture
/// (for its mask). None when nothing could be drawn.
fn draw_item(
    pix: &mut Pixmap,
    item: &Item,
    timeline: &Timeline,
    store: &dyn MediaStore,
    t: u64,
    texts: &mut TextCache,
    pictures: &mut PictureCache,
) -> Option<(Rect, Transform)> {
    let (w, h) = (pix.width() as f32, pix.height() as f32);
    let m = motion(item, t, w, h);
    match &item.what {
        What::Media { id } => {
            let m_item = timeline.media.iter().find(|x| x.id == *id)?;
            let at = if m_item.kind == MediaKind::Picture { 0 } else { item.from + (t - item.at) };
            let pic = store.picture(*id, at)?;
            let ready = pictures.get(*id, &pic)?;
            let rect = media_rect(item, pic.width(), pic.height(), w, h);
            if item.look.is_plain() {
                picture_at(pix, &ready, rect, m);
            } else {
                framed_at(pix, &ready, rect, m, &item.look);
            }
            let (k, turn) = (m.scale.max(0.01), item.look.clone().checked().rotate);
            let to = Transform::from_scale(k, k).post_rotate(turn).post_translate(rect.x + rect.w / 2.0 + m.dx, rect.y + rect.h / 2.0 + m.dy);
            Some((Rect::new(-rect.w / 2.0, -rect.h / 2.0, rect.w, rect.h), to))
        }
        What::Text { style: TextStyle::Blur, title, subtitle, color } => {
            // What's under its box blurred (it draws nothing of its own), coming on and going
            // as the graphics do, and as the item's own animations and opacity say.
            let g = graphic(Some(&mut *texts), TextStyle::Blur, title, subtitle, color, item.len, item.place.w)?;
            let amount = crate::stock::blur_amount(item.len, t.saturating_sub(item.at) as f32 / 1e6) * m.alpha;
            let r = moved(g.bounds, stock_place(TextStyle::Blur, &g, item.place, w, h));
            // Moved and grown about the middle of the picture, as a graphic's picture is.
            let (k, cx, cy) = (m.scale.max(0.01), w / 2.0, h / 2.0);
            let r = Rect::new((r.x - cx) * k + cx + m.dx, (r.y - cy) * k + cy + m.dy, r.w * k, r.h * k);
            blur_box(pix, r, amount);
            Some((r, Transform::identity()))
        }
        What::Text { style, title, subtitle, color } if crate::stock::is_stock(*style) => {
            // A broadcast graphic: played, a frame for each moment of its item.
            let g = graphic(Some(&mut *texts), *style, title, subtitle, color, item.len, item.place.w)?;
            let secs = (t - item.at) as f32 / 1e6;
            let at = stock_place(*style, &g, item.place, w, h);
            let turn = item.look.clone().checked().rotate;
            let r = moved(g.bounds, at);
            let pivot = Pt::new(r.x + r.w / 2.0, r.y + r.h / 2.0);
            let plain = m.alpha >= 0.999
                && item.place.opacity >= 0.999
                && m.dx == 0.0
                && m.dy == 0.0
                && (m.scale - 1.0).abs() < 1.0e-4
                && turn.abs() < 1.0e-3;
            if plain {
                g.animation.render(pix, secs, at);
            } else if let Some(mut own) = Pixmap::new(w as u32, h as u32) {
                g.animation.render(&mut own, secs, at);
                turned_at(pix, &own, Rect::new(0.0, 0.0, w, h), Motion { dx: m.dx, dy: m.dy, scale: m.scale, alpha: m.alpha }, turn, pivot);
            }
            Some((r, frame_transform(m, w, h, turn, pivot)))
        }
        What::Text { style, title, subtitle, color } => {
            let key = format!("{style:?}|{title}|{subtitle}|{color}|{:?}|{}x{}", item.place, w, h);
            let (st, ti, su, co, place) = (*style, title.as_str(), subtitle.as_str(), color.as_str(), item.place);
            let drawn = texts.get(key, || {
                let mut p = Pixmap::new(w as u32, h as u32)?;
                text_item(Some(&mut p), st, ti, su, co, place, w, h);
                Some(p)
            })?;
            let r = text_item(None, st, ti, su, co, place, w, h);
            let pivot = Pt::new(r.x + r.w / 2.0, r.y + r.h / 2.0);
            let turn = item.look.clone().checked().rotate;
            turned_at(pix, &drawn, Rect::new(0.0, 0.0, w, h), Motion { dx: m.dx, dy: m.dy, scale: m.scale, alpha: m.alpha }, turn, pivot);
            Some((r, frame_transform(m, w, h, turn, pivot)))
        }
        What::Transition { kind } => {
            let p = (t - item.at) as f32 / item.len.max(1) as f32;
            transition(pix, *kind, p);
            None
        }
        // It zooms the screen under everything (see `crate::zoom`); it draws nothing itself.
        What::Zoom => None,
    }
}

/// Mixes the sound layers' items (and the sound of recordings placed on picture layers) into
/// `out` (`count` stereo samples from timeline sample `first`).
pub fn mix_layers(out: &mut [f32], timeline: &Timeline, store: &dyn MediaStore, first: u64, count: usize) {
    use crate::compose::to_samples;
    let end = first.saturating_add(count as u64);
    for l in &timeline.layers {
        if l.hidden || l.volume <= 0.0 {
            continue;
        }
        for item in &l.items {
            let What::Media { id } = &item.what else { continue };
            let Some(m) = timeline.media.iter().find(|m| m.id == *id) else { continue };
            if m.kind == MediaKind::Picture {
                continue;
            }
            let (at, len, from) = (to_samples(item.at), to_samples(item.len), to_samples(item.from));
            let (a, b) = (first.max(at), end.min(at.saturating_add(len)));
            if a >= b {
                continue;
            }
            let n = (b - a) as usize;
            let src = store.sound(*id, from + (a - at), n);
            let base = (a - first) as usize * 2;
            // Faded in and out with the item, as its picture is.
            for k in 0..n {
                let tt = crate::compose::to_us(a + k as u64);
                let (inn, outp) = item.anim_at(tt);
                let fade = |kind: AnimKind, p: f32| if kind == AnimKind::None { 1.0 } else { p };
                let g = l.volume * fade(item.intro.kind, inn) * (1.0 - if item.outro.kind == AnimKind::None { 0.0 } else { outp });
                if let (Some(o), Some(s)) = (out.get_mut(base + k * 2..base + k * 2 + 2), src.get(k * 2..k * 2 + 2)) {
                    o[0] += s[0] * g;
                    o[1] += s[1] * g;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{Placement, template};
    use crate::recording::StreamKind;
    use crate::timeline::{CameraLayout, CaptionOptions, Clip, Track};

    fn timeline() -> Timeline {
        Timeline {
            tracks: vec![Track {
                stream: 0,
                kind: StreamKind::Screen,
                muted: false,
                volume: 1.0,
                clips: vec![Clip::new(0, 0, 10_000_000)],
                name: None,
                denoise: 0.0,
                points: Vec::new(),
                locked: false,
                solo: false,
            }],
            camera: CameraLayout::default(),
            captions: CaptionOptions::default(),
            layers: Vec::new(),
            media: Vec::new(),
            pointer: Default::default(),
            corrected: None,
            markers: Vec::new(),
            keys: Default::default(),
            ducking: Default::default(),
            watermark: Default::default(),
            frame: Default::default(),
            motion_blur: Default::default(),
            pointer_hidden: Vec::new(),
        }
    }

    fn grey(w: u32, h: u32) -> Raster {
        Raster::filled(w, h, Color::rgb(120, 120, 120)).unwrap()
    }

    /// Black and white squares `side` pixels across.
    fn checks(w: u32, h: u32, side: u32) -> Raster {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let v = if (x / side + y / side).is_multiple_of(2) { 0 } else { 255 };
                px.extend_from_slice(&[v, v, v, 255]);
            }
        }
        Raster::new(w, h, px).unwrap()
    }

    fn blur_item(place: Placement, len: u64) -> Item {
        Item { place, len, ..template("blur").unwrap().item }
    }

    #[test]
    fn every_library_item_draws_something_and_says_where() {
        for t in layers::library() {
            let mut tl = timeline();
            let (_, _) = tl.place(t.item.clone(), None, 0).unwrap();
            // A blur draws nothing of its own: it shows on a picture with something in it.
            let blur = matches!(t.item.what, What::Text { style: TextStyle::Blur, .. });
            let back = || if blur { checks(320, 180, 4) } else { grey(320, 180) };
            let mut out = back();
            let mid = t.item.len / 2;
            draw_layers(&mut out, &tl, &NoMedia, mid, &mut TextCache::default(), &mut PictureCache::default());
            if matches!(t.item.what, What::Zoom) {
                // A zoom draws nothing: it zooms the screen under everything (see `crate::zoom`).
                assert_eq!(out, back(), "{} draws nothing itself", t.key);
                assert!(!crate::zoom::view_at(&tl, mid).is_whole(), "{} zooms", t.key);
            } else {
                assert_ne!(out, back(), "{} changes the picture", t.key);
            }
            let item = &tl.layers[0].items[0];
            let rect = item_rect(item, &tl, &NoMedia, 320.0, 180.0);
            assert_eq!(rect.is_none(), matches!(item.what, What::Transition { .. }), "{}", t.key);
        }
    }

    #[test]
    fn a_title_fades_in_and_a_dip_is_darkest_in_its_middle() {
        let mut tl = timeline();
        tl.place(template("dip-black").unwrap().item, None, 0).unwrap();
        let at = |t: u64| {
            let mut out = grey(64, 36);
            draw_layers(&mut out, &tl, &NoMedia, t, &mut TextCache::default(), &mut PictureCache::default());
            out.pixel(32, 18).unwrap().r
        };
        assert!(at(500_000) < 10, "black in the middle");
        assert!(at(100_000) > at(400_000), "darker towards the middle");
        let title = Item { intro: layers::Anim { kind: AnimKind::Fade, len: 1_000_000 }, ..template("title").unwrap().item };
        assert_eq!(motion(&title, 0, 100.0, 100.0).alpha, 0.0);
        assert!(motion(&title, 500_000, 100.0, 100.0).alpha > 0.3);
        assert_eq!(motion(&title, 2_000_000, 100.0, 100.0).alpha, 1.0);
    }

    struct OnePicture(Arc<Raster>);

    impl MediaStore for OnePicture {
        fn picture(&self, _id: u32, _t: u64) -> Option<Arc<Raster>> {
            Some(self.0.clone())
        }
        fn sound(&self, _id: u32, _first: u64, count: usize) -> Vec<f32> {
            vec![0.5; count * 2]
        }
    }

    #[test]
    fn a_picture_goes_where_it_was_placed_and_a_sound_is_mixed_in() {
        let mut tl = timeline();
        let pic = tl
            .add_media(layers::MediaItem { name: "Logo.png".into(), path: "C:/Logo.png".into(), width: 10, height: 10, ..Default::default() })
            .unwrap();
        let item = Item {
            what: What::Media { id: pic },
            place: Placement { x: 0.25, y: 0.5, w: 0.25, opacity: 1.0 },
            intro: layers::Anim { kind: AnimKind::None, len: 0 },
            ..Item::default()
        };
        tl.place(item, None, 0).unwrap();
        let red = Arc::new(Raster::filled(10, 10, Color::rgb(255, 0, 0)).unwrap());
        let mut out = grey(100, 100);
        draw_layers(&mut out, &tl, &OnePicture(red.clone()), 1_000_000, &mut TextCache::default(), &mut PictureCache::default());
        assert_eq!(out.pixel(25, 50).unwrap().r, 255, "in its place");
        assert_eq!(out.pixel(75, 50).unwrap().r, 120, "and nowhere else");
        let song = tl
            .add_media(layers::MediaItem {
                name: "Song.wav".into(),
                path: "C:/Song.wav".into(),
                kind: MediaKind::Sound,
                duration: 9_000_000,
                ..Default::default()
            })
            .unwrap();
        let s = Item {
            what: What::Media { id: song },
            intro: layers::Anim { kind: AnimKind::None, len: 0 },
            outro: layers::Anim { kind: AnimKind::None, len: 0 },
            ..Item::default()
        };
        tl.place(s, None, 0).unwrap();
        let mut mixed = vec![0.0f32; 20];
        mix_layers(&mut mixed, &tl, &OnePicture(red), 48_000, 10);
        assert!(mixed.iter().all(|v| (*v - 0.5).abs() < 1e-6), "{mixed:?}");
    }

    #[test]
    fn a_picture_is_cut_down_turned_bordered_rounded_and_shadowed() {
        let mut tl = timeline();
        let pic = tl
            .add_media(layers::MediaItem { name: "Wide.png".into(), path: "C:/Wide.png".into(), width: 40, height: 20, ..Default::default() })
            .unwrap();
        // Its left half red, its right half blue; the right half cut off: a red square.
        let mut wide = Raster::filled(40, 20, Color::rgb(255, 0, 0)).unwrap();
        for y in 0..20 {
            for x in 20..40 {
                let i = ((y * 40 + x) * 4) as usize;
                wide.pixels_mut()[i..i + 3].copy_from_slice(&[0, 0, 255]);
            }
        }
        let look = layers::ItemLook { crop: [0.25, 0.0, 0.25, 0.0], ..layers::ItemLook::default() };
        let item = Item {
            what: What::Media { id: pic },
            place: Placement { x: 0.5, y: 0.5, w: 0.4, opacity: 1.0 },
            intro: layers::Anim { kind: AnimKind::None, len: 0 },
            look: look.clone(),
            ..Item::default()
        };
        let (layer, id) = tl.place(item, None, 0).unwrap();
        let draw = |tl: &Timeline| {
            let mut out = grey(100, 100);
            draw_layers(&mut out, tl, &OnePicture(Arc::new(wide.clone())), 1_000_000, &mut TextCache::default(), &mut PictureCache::default());
            out
        };
        let out = draw(&tl);
        let r = |x, y| out.pixel(x, y).unwrap();
        assert_eq!(media_rect(tl.item(layer, id).unwrap(), 40, 20, 100.0, 100.0), Rect::new(30.0, 30.0, 40.0, 40.0), "square, as cut");
        assert!(r(50, 50).r > 200 && r(50, 50).b < 40, "{:?}", r(50, 50));
        assert!(r(65, 35).r > 200, "no blue left: {:?}", r(65, 35));
        assert_eq!(r(80, 50), Color::rgb(120, 120, 120), "nothing beyond it");
        // A border, round corners and a shadow.
        tl.set_item(layer, id, |i| {
            i.look = layers::ItemLook { border: 0.05, border_colour: "#00ff00".into(), radius: 0.5, shadow: true, ..look.clone() }
        })
        .unwrap();
        let out = draw(&tl);
        let r = |x, y| out.pixel(x, y).unwrap();
        assert!(r(50, 30).g > 200 && r(50, 30).r < 80, "the border at its top: {:?}", r(50, 30));
        assert_eq!(r(31, 31), Color::rgb(120, 120, 120), "a round corner leaves the corner");
        assert!(r(50, 72).r < 110, "the shadow under it: {:?}", r(50, 72));
        // Turned a quarter: a square stays where it was.
        tl.set_item(layer, id, |i| i.look = layers::ItemLook { rotate: 90.0, ..look.clone() }).unwrap();
        let out = draw(&tl);
        assert!(out.pixel(35, 35).unwrap().r > 200 && out.pixel(25, 50).unwrap() == Color::rgb(120, 120, 120));
        // Hostile looks are made sensible.
        let wild = layers::ItemLook {
            rotate: f32::NAN,
            crop: [9.0, -1.0, f32::INFINITY, 0.3],
            border: 7.0,
            border_colour: "red".into(),
            radius: -2.0,
            shadow: true,
            mask: layers::Mask { shape: layers::MaskShape::Star, x: f32::NAN, y: 9.0, w: -1.0, h: f32::INFINITY, cut: true },
            ..layers::ItemLook::default()
        }
        .checked();
        assert_eq!(
            (wild.rotate, wild.crop, wild.border, wild.border_colour.as_str(), wild.radius),
            (0.0, [0.45, 0.0, 0.0, 0.3], 0.1, "#ffffff", 0.0)
        );
        assert_eq!((wild.mask.x, wild.mask.y, wild.mask.w, wild.mask.h), (0.5, 1.5, 0.05, 1.0));
        tl.set_item(layer, id, |i| i.look = wild.clone()).unwrap();
        let _ = draw(&tl);
    }

    #[test]
    fn a_sliding_picture_is_blurred_along_its_way_and_a_still_one_is_not() {
        let mut tl = timeline();
        let pic = tl
            .add_media(layers::MediaItem { name: "Square.png".into(), path: "C:/Square.png".into(), width: 20, height: 20, ..Default::default() })
            .unwrap();
        let red = Raster::filled(20, 20, Color::rgb(200, 0, 0)).unwrap();
        // Sliding in from the left over its first second.
        let item = Item {
            what: What::Media { id: pic },
            place: Placement { x: 0.5, y: 0.5, w: 0.2, opacity: 1.0 },
            intro: layers::Anim { kind: AnimKind::SlideLeft, len: 1_000_000 },
            outro: layers::Anim { kind: AnimKind::None, len: 0 },
            len: 3_000_000,
            ..Item::default()
        };
        tl.place(item, None, 0).unwrap();
        let draw = |t: u64, blur: u64| {
            let mut out = grey(200, 100);
            draw_layers_at(&mut out, &tl, &OnePicture(Arc::new(red.clone())), t, blur, &mut TextCache::default(), &mut PictureCache::default());
            out
        };
        // Pixels part red, part grey: the blur's smear at its edges.
        let smeared = |p: &Raster| p.pixels().chunks(4).filter(|c| c[0] > 125 && c[0] < 195 && c[1] < 115).count();
        let (sharp, soft) = (draw(500_000, 0), draw(500_000, 40_000));
        assert!(smeared(&soft) > smeared(&sharp) + 20, "{} {}", smeared(&soft), smeared(&sharp));
        // Still (after its slide), it's drawn as it is.
        assert_eq!(draw(2_000_000, 40_000), draw(2_000_000, 0));
        // At its very start the moments before it are kept to it.
        let _ = draw(0, 40_000);
    }

    #[test]
    fn a_picture_on_a_layer_has_a_device_drawn_round_it() {
        let mut tl = timeline();
        let pic = tl
            .add_media(layers::MediaItem { name: "Tall.png".into(), path: "C:/Tall.png".into(), width: 20, height: 40, ..Default::default() })
            .unwrap();
        let red = Raster::filled(20, 40, Color::rgb(200, 0, 0)).unwrap();
        let item = Item {
            what: What::Media { id: pic },
            place: Placement { x: 0.5, y: 0.5, w: 0.2, opacity: 1.0 },
            intro: layers::Anim { kind: AnimKind::None, len: 0 },
            look: layers::ItemLook { device: crate::frame::Device::Phone, ..layers::ItemLook::default() },
            ..Item::default()
        };
        tl.place(item, None, 0).unwrap();
        let mut out = grey(100, 100);
        draw_layers(&mut out, &tl, &OnePicture(Arc::new(red)), 1_000_000, &mut TextCache::default(), &mut PictureCache::default());
        // The picture covers 40 to 60 across, 30 to 70 down; the phone's dark body round it, its
        // thick ends above and below.
        assert_eq!(out.pixel(50, 50).unwrap(), Color::rgb(200, 0, 0));
        let body = out.pixel(50, 28).unwrap();
        assert!(body.r < 60 && body.g < 60, "{body:?}");
        let side = out.pixel(39, 50).unwrap();
        assert!(side.r < 60, "{side:?}");
        assert_eq!(out.pixel(20, 50).unwrap(), Color::rgb(120, 120, 120));
        // Its corners rounded inside the phone.
        assert_ne!(out.pixel(40, 30).unwrap(), Color::rgb(200, 0, 0));
    }

    #[test]
    fn a_picture_is_masked_by_a_shape_and_blended_with_what_is_under_it() {
        let mut tl = timeline();
        let pic = tl
            .add_media(layers::MediaItem { name: "Square.png".into(), path: "C:/Square.png".into(), width: 40, height: 40, ..Default::default() })
            .unwrap();
        let red = Raster::filled(40, 40, Color::rgb(200, 0, 0)).unwrap();
        let item = Item {
            what: What::Media { id: pic },
            place: Placement { x: 0.5, y: 0.5, w: 0.4, opacity: 1.0 },
            intro: layers::Anim { kind: AnimKind::None, len: 0 },
            ..Item::default()
        };
        let (layer, id) = tl.place(item, None, 0).unwrap();
        let draw = |tl: &Timeline| {
            let mut out = grey(100, 100);
            draw_layers(&mut out, tl, &OnePicture(Arc::new(red.clone())), 1_000_000, &mut TextCache::default(), &mut PictureCache::default());
            out
        };
        let set = |tl: &mut Timeline, look: layers::ItemLook| tl.set_item(layer, id, |i| i.look = look).unwrap();
        let grey_ = Color::rgb(120, 120, 120);
        // It covers 30 to 70 each way. An oval mask: its middle shown, its corners not.
        let oval = layers::Mask { shape: layers::MaskShape::Ellipse, ..layers::Mask::default() };
        set(&mut tl, layers::ItemLook { mask: oval, ..layers::ItemLook::default() });
        let out = draw(&tl);
        assert_eq!(out.pixel(50, 50).unwrap(), Color::rgb(200, 0, 0));
        assert_eq!(out.pixel(32, 32).unwrap(), grey_, "the corner is outside the oval");
        // The oval cut out of it: the opposite.
        set(&mut tl, layers::ItemLook { mask: layers::Mask { cut: true, ..oval }, ..layers::ItemLook::default() });
        let out = draw(&tl);
        assert_eq!(out.pixel(50, 50).unwrap(), grey_);
        assert_eq!(out.pixel(32, 32).unwrap(), Color::rgb(200, 0, 0));
        assert_eq!(out.pixel(10, 10).unwrap(), grey_, "nothing beyond the item");
        // A small rectangle in its top left quarter: only that.
        let corner = layers::Mask { shape: layers::MaskShape::Rectangle, x: 0.25, y: 0.25, w: 0.5, h: 0.5, cut: false };
        set(&mut tl, layers::ItemLook { mask: corner, ..layers::ItemLook::default() });
        let out = draw(&tl);
        assert_eq!(out.pixel(35, 35).unwrap(), Color::rgb(200, 0, 0));
        assert_eq!(out.pixel(60, 60).unwrap(), grey_);
        // Every shape draws something in its middle, and nothing far outside the item.
        for shape in layers::MaskShape::ALL {
            set(&mut tl, layers::ItemLook { mask: layers::Mask { shape, ..layers::Mask::default() }, ..layers::ItemLook::default() });
            let out = draw(&tl);
            assert_eq!(out.pixel(50, 55).unwrap(), Color::rgb(200, 0, 0), "{shape:?}");
            assert_eq!(out.pixel(20, 50).unwrap(), grey_, "{shape:?}");
        }
        // Blends: multiply darkens, screen lightens, darken and lighten pick.
        let at = |tl: &mut Timeline, blend| {
            set(tl, layers::ItemLook { blend, ..layers::ItemLook::default() });
            draw(tl).pixel(50, 50).unwrap()
        };
        assert_eq!(at(&mut tl, layers::BlendMode::Normal), Color::rgb(200, 0, 0));
        let p = at(&mut tl, layers::BlendMode::Multiply);
        assert!((92..=96).contains(&p.r) && p.g == 0 && p.b == 0, "multiply: {p:?}");
        let p = at(&mut tl, layers::BlendMode::Screen);
        assert!(p.r > 200 && (118..=122).contains(&p.g), "screen: {p:?}");
        let p = at(&mut tl, layers::BlendMode::Darken);
        assert!((118..=122).contains(&p.r) && p.g == 0, "darken: {p:?}");
        let p = at(&mut tl, layers::BlendMode::Lighten);
        assert!(p.r >= 199 && (118..=122).contains(&p.g), "lighten: {p:?}");
        let p = at(&mut tl, layers::BlendMode::Overlay);
        assert!(p.r > 120 && p.g < 120, "overlay: {p:?}");
        // A mask and a blend on words as well, turned.
        let (tl2, _) = {
            let mut tl2 = timeline();
            let words = Item {
                what: What::Text { style: TextStyle::TitleBox, title: "Masked".into(), subtitle: String::new(), color: String::new() },
                intro: layers::Anim { kind: AnimKind::None, len: 0 },
                look: layers::ItemLook { rotate: 30.0, blend: layers::BlendMode::Multiply, mask: oval, ..layers::ItemLook::default() },
                ..Item::default()
            };
            let placed = tl2.place(words, None, 0).unwrap();
            (tl2, placed)
        };
        let mut out = grey(160, 90);
        draw_layers(&mut out, &tl2, &NoMedia, 1_000_000, &mut TextCache::default(), &mut PictureCache::default());
        assert!(out.pixels().iter().step_by(4).any(|r| *r < 110), "the box multiplied over the grey");
    }

    #[test]
    fn a_blur_hides_what_is_under_its_box_and_nothing_else() {
        let (w, h) = (640u32, 360u32);
        let mut tl = timeline();
        let (layer, id) = tl.place(blur_item(Placement { x: 0.5, y: 0.5, w: 0.25, opacity: 1.0 }, 4_000_000), None, 0).unwrap();
        let item = tl.item(layer, id).unwrap().clone();
        let at = |t: u64| {
            let mut out = checks(w, h, 3);
            draw_layers(&mut out, &tl, &NoMedia, t, &mut TextCache::default(), &mut PictureCache::default());
            out
        };
        // Its box, where the designer puts its handles: a quarter of the width, in the middle.
        let r = item_rect(&item, &tl, &NoMedia, w as f32, h as f32).unwrap();
        assert!((r.w - w as f32 * 0.25).abs() < 1.0 && (r.x + r.w / 2.0 - w as f32 / 2.0).abs() < 1.0, "{r:?}");
        let mid = at(2_000_000);
        // Inside: grey, the squares gone (the most and least of a patch in its middle close).
        let (cx, cy) = (w / 2, h / 2);
        let patch: Vec<u8> =
            (cy - 10..cy + 10).flat_map(|y| (cx - 10..cx + 10).map(move |x| (x, y))).map(|(x, y)| mid.pixel(x, y).unwrap().r).collect();
        let (lo, hi) = (*patch.iter().min().unwrap(), *patch.iter().max().unwrap());
        assert!(lo > 90 && hi < 165 && hi - lo < 30, "blurred to grey: {lo}..{hi}");
        // Well outside it: as it was.
        let before = checks(w, h, 3);
        for (x, y) in [(10, 10), (w - 10, h / 2), (w / 2, 8), (60, h - 20), (w / 2 - (r.w as u32) / 2 - 20, h / 2)] {
            assert_eq!(mid.pixel(x, y), before.pixel(x, y), "({x}, {y}) is left alone");
        }
        // It comes on and goes: nothing at its very start and at its very end, all of it between.
        assert_eq!(at(0), before, "nothing yet at its start");
        assert_eq!(at(3_999_999), before, "nothing left at its end");
        let soon = at(150_000).pixel(cx, cy).unwrap().r;
        assert!(soon != 0 && soon != 255 && (soon as i32 - 128).abs() > (mid.pixel(cx, cy).unwrap().r as i32 - 128).abs(), "part-blurred: {soon}");
    }

    #[test]
    fn an_arrow_keeps_its_tip_where_it_was_put_at_any_size() {
        let g = crate::stock::build(TextStyle::Arrow, "Click here", "", [255, 59, 48], 4_000_000, 1.0).unwrap();
        for grow in [0.5, 1.0, 2.5] {
            let place = Placement { x: 0.62, y: 0.42, w: grow, opacity: 1.0 };
            let t = stock_place(TextStyle::Arrow, &g, place, 1280.0, 720.0);
            let (x, y) = (g.anchor.0 * t.sx + t.tx, g.anchor.1 * t.sy + t.ty);
            assert!((x - 0.62 * 1280.0).abs() < 0.5 && (y - 0.42 * 720.0).abs() < 0.5, "{grow}: ({x}, {y})");
        }
    }

    #[test]
    fn a_blur_placed_anyhow_never_panics() {
        let junk = [
            Placement { x: f32::NAN, y: f32::NAN, w: f32::NAN, opacity: f32::NAN },
            Placement { x: f32::INFINITY, y: f32::NEG_INFINITY, w: f32::MAX, opacity: 1.0 },
            Placement { x: -1.0e30, y: 1.0e30, w: -5.0, opacity: -3.0 },
            Placement { x: 0.0, y: 0.0, w: 3.0, opacity: 9.0 },
            Placement { x: 1.5, y: -0.5, w: 0.02, opacity: 1.0 },
            Placement { x: 0.999, y: 0.999, w: 1.0e-9, opacity: 1.0 },
        ];
        for place in junk {
            for len in [1, 300_000, 4_000_000, u64::MAX / 8, u64::MAX] {
                let item = Item { at: 0, ..blur_item(place, len) };
                // As a file might hold it (nothing made safe), and as the timeline places it.
                let mut raw = timeline();
                raw.layers = vec![layers::Layer { id: 1, items: vec![item.clone()], ..Default::default() }];
                let mut placed = timeline();
                placed.place(item, None, 0).unwrap();
                for tl in [&raw, &placed] {
                    for t in [0, 1, 200_000, 2_000_000, u64::MAX / 16] {
                        for (w, h) in [(64, 36), (1, 1), (3, 200)] {
                            let mut out = checks(w, h, 2);
                            draw_layers(&mut out, tl, &NoMedia, t, &mut TextCache::default(), &mut PictureCache::default());
                            let _ = item_rect(&tl.layers[0].items[0], tl, &NoMedia, w as f32, h as f32);
                        }
                    }
                }
            }
        }
        // And straight on a picture: boxes that are empty, outside it, or junk.
        let mut pix = Pixmap::new(8, 8).unwrap();
        for r in [
            Rect::new(f32::NAN, 0.0, 4.0, 4.0),
            Rect::new(-50.0, -50.0, 10.0, 10.0),
            Rect::new(2.0, 2.0, 0.0, 0.0),
            Rect::new(6.0, 6.0, -9.0, -9.0),
            Rect::new(0.0, 0.0, 1e30, 1e30),
        ] {
            blur_box(&mut pix, r, 1.0);
            blur_box(&mut pix, r, f32::NAN);
        }
    }
}
