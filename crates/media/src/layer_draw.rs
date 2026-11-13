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
    Transform::from_translate(-cx, -cy)
        .post_scale(grow, grow)
        .post_translate(cx + place.x * W - g.anchor.0, cy + place.y * H - g.anchor.1)
        .post_scale(k, k)
        .post_translate(0.0, (h - H * k) * (cy / H))
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

/// Where a media item's picture of `pw × ph` goes on a `w × h` video (before its motion).
pub fn media_rect(item: &Item, pw: u32, ph: u32, w: f32, h: f32) -> Rect {
    let bw = (item.place.w * w).max(2.0);
    let bh = bw * ph.max(1) as f32 / pw.max(1) as f32;
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
    if m.alpha <= 0.0 || pic.width() == 0 || pic.height() == 0 {
        return;
    }
    let (sx, sy) = (rect.w / pic.width() as f32, rect.h / pic.height() as f32);
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let t = Transform::from_scale(sx, sy)
        .post_translate(rect.x, rect.y)
        .post_translate(-cx, -cy)
        .post_scale(m.scale.max(0.01), m.scale.max(0.01))
        .post_translate(cx + m.dx, cy + m.dy);
    let paint = PixmapPaint { opacity: m.alpha, quality: tiny_skia::FilterQuality::Bilinear, ..PixmapPaint::default() };
    out.draw_pixmap(0, 0, pic.as_ref(), &paint, t, None);
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
/// the later layers on top.
pub fn draw_layers(out: &mut Raster, timeline: &Timeline, store: &dyn MediaStore, t: u64, texts: &mut TextCache, pictures: &mut PictureCache) {
    let showing: Vec<&Item> = timeline
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Picture && !l.hidden)
        .filter_map(|l| l.items.iter().find(|i| i.at <= t && t < i.end()))
        .collect();
    if showing.is_empty() {
        return;
    }
    let Some(mut pix) = canvas::to_pixmap(out) else { return };
    let (w, h) = (pix.width() as f32, pix.height() as f32);
    for item in showing {
        let m = motion(item, t, w, h);
        match &item.what {
            What::Media { id } => {
                let Some(m_item) = timeline.media.iter().find(|x| x.id == *id) else { continue };
                let at = if m_item.kind == MediaKind::Picture { 0 } else { item.from + (t - item.at) };
                let Some(pic) = store.picture(*id, at) else { continue };
                let Some(ready) = pictures.get(*id, &pic) else { continue };
                picture_at(&mut pix, &ready, media_rect(item, pic.width(), pic.height(), w, h), m);
            }
            What::Text { style, title, subtitle, color } if crate::stock::is_stock(*style) => {
                // A broadcast graphic: played, a frame for each moment of its item.
                let Some(g) = graphic(Some(&mut *texts), *style, title, subtitle, color, item.len, item.place.w) else { continue };
                let secs = (t - item.at) as f32 / 1e6;
                let at = stock_place(*style, &g, item.place, w, h);
                let plain = m.alpha >= 0.999 && item.place.opacity >= 0.999 && m.dx == 0.0 && m.dy == 0.0 && (m.scale - 1.0).abs() < 1.0e-4;
                if plain {
                    g.animation.render(&mut pix, secs, at);
                } else if let Some(mut own) = Pixmap::new(w as u32, h as u32) {
                    g.animation.render(&mut own, secs, at);
                    picture_at(&mut pix, &own, Rect::new(0.0, 0.0, w, h), Motion { dx: m.dx, dy: m.dy, scale: m.scale, alpha: m.alpha });
                }
            }
            What::Text { style, title, subtitle, color } => {
                let key = format!("{style:?}|{title}|{subtitle}|{color}|{:?}|{}x{}", item.place, w, h);
                let (st, ti, su, co, place) = (*style, title.as_str(), subtitle.as_str(), color.as_str(), item.place);
                let drawn = texts.get(key, || {
                    let mut p = Pixmap::new(w as u32, h as u32)?;
                    text_item(Some(&mut p), st, ti, su, co, place, w, h);
                    Some(p)
                });
                if let Some(p) = drawn {
                    picture_at(&mut pix, &p, Rect::new(0.0, 0.0, w, h), Motion { dx: m.dx, dy: m.dy, scale: m.scale, alpha: m.alpha });
                }
            }
            What::Transition { kind } => {
                let p = (t - item.at) as f32 / item.len.max(1) as f32;
                transition(&mut pix, *kind, p);
            }
        }
    }
    if let Some(r) = canvas::from_pixmap(&pix) {
        *out = r;
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
                clips: vec![Clip { at: 0, from: 0, len: 10_000_000 }],
            }],
            camera: CameraLayout::default(),
            captions: CaptionOptions::default(),
            layers: Vec::new(),
            media: Vec::new(),
            pointer: Default::default(),
        }
    }

    fn grey(w: u32, h: u32) -> Raster {
        Raster::filled(w, h, Color::rgb(120, 120, 120)).unwrap()
    }

    #[test]
    fn every_library_item_draws_something_and_says_where() {
        for t in layers::library() {
            let mut tl = timeline();
            let (_, _) = tl.place(t.item.clone(), None, 0).unwrap();
            let mut out = grey(320, 180);
            let mid = t.item.len / 2;
            draw_layers(&mut out, &tl, &NoMedia, mid, &mut TextCache::default(), &mut PictureCache::default());
            assert_ne!(out, grey(320, 180), "{} changes the picture", t.key);
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
}
