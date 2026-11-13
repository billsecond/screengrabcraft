//! The library's graphics: lower thirds, broadcast pieces (a live bug, a ticker, tags, cards),
//! titles, callouts and overlays, and full-screen stinger transitions. Each is built here as a
//! Lottie animation (see `screengrabcraft_render::lottie`, which plays them: the same player
//! plays the Lottie files brought in from elsewhere), sized to its words and as long as its
//! item: it comes on over its first moments, stays, and goes over its last.
//!
//! How they're made to look: plates with slanted or rounded ends, shaded top to bottom, a soft
//! shadow under them, a shine that crosses once, heavy lettering set a little apart, and words
//! that arrive after their plate, always on top of it.
//!
//! All of it is original: shapes, lines and words drawn by this code. Nothing here is taken
//! from any broadcaster's or program's graphics.

use screengrabcraft_model::{Rect, Style};
use screengrabcraft_render::lottie::Animation;
use screengrabcraft_render::text;
use serde_json::{Value, json};

use crate::layers::TextStyle;

/// The size the graphics are designed at (they're scaled to the video's).
pub const W: f32 = 1920.0;
pub const H: f32 = 1080.0;
const FPS: f32 = 30.0;
/// How long a graphic takes to go (frames).
const OUT: f32 = 12.0;

type Rgb = [f32; 3];
type P = [f32; 2];
const WHITE: Rgb = [1.0, 1.0, 1.0];
const OFF: Rgb = [0.90, 0.92, 0.95];
const INK: Rgb = [0.055, 0.063, 0.086];
const DARK: Rgb = [0.055, 0.063, 0.086];
const DARK2: Rgb = [0.125, 0.141, 0.188];
const SOFT: Rgb = [0.78, 0.81, 0.86];
const BLACK: Rgb = [0.0, 0.0, 0.0];

/// How a value gets from one keyframe to the next.
#[derive(Clone, Copy)]
enum E {
    /// Quick, then settling.
    Out,
    /// Slow, then quick.
    In,
    Lin,
    /// Past its mark and back: a pop.
    Back,
}

fn handles(e: E) -> (P, P) {
    match e {
        E::Out => ([0.2, 1.0], [0.3, 1.0]),
        E::In => ([0.7, 0.0], [0.8, 0.0]),
        E::Lin => ([0.0, 0.0], [1.0, 1.0]),
        E::Back => ([0.34, 1.56], [0.64, 1.0]),
    }
}

/// An animated property: each value with its frame and how it moves on to the next.
fn keys(k: &[(f32, Vec<f32>, E)]) -> Value {
    let mut out = Vec::with_capacity(k.len());
    let mut last = f32::MIN;
    for (t, v, e) in k {
        // Frames in order, whatever the lengths asked for.
        let t = t.max(last + 0.01);
        last = t;
        let (o, i) = handles(*e);
        out.push(json!({"t": t, "s": v, "o": {"x": [o[0]], "y": [o[1]]}, "i": {"x": [i[0]], "y": [i[1]]}}));
    }
    json!({"a": 1, "k": out})
}

fn fixed(v: &[f32]) -> Value {
    json!({"a": 0, "k": v})
}

/// An outline through points (closed or not), as a path's value.
fn outline(pts: &[P], closed: bool) -> Value {
    let zero: Vec<P> = pts.iter().map(|_| [0.0, 0.0]).collect();
    json!({"v": pts, "i": zero, "o": zero, "c": closed})
}

/// A path that changes shape: each outline with its frame and how it moves on.
fn outlines(k: &[(f32, Vec<P>, E)]) -> Value {
    let mut out = Vec::with_capacity(k.len());
    let mut last = f32::MIN;
    for (t, pts, e) in k {
        let t = t.max(last + 0.01);
        last = t;
        let (o, i) = handles(*e);
        out.push(json!({"t": t, "s": [outline(pts, true)], "o": {"x": [o[0]], "y": [o[1]]}, "i": {"x": [i[0]], "y": [i[1]]}}));
    }
    json!({"ty": "sh", "ks": {"a": 1, "k": out}})
}

fn rgb(c: [u8; 3]) -> Rgb {
    [f32::from(c[0]) / 255.0, f32::from(c[1]) / 255.0, f32::from(c[2]) / 255.0]
}

/// A darker shade of a colour.
fn shade(c: Rgb, k: f32) -> Rgb {
    [c[0] * k, c[1] * k, c[2] * k]
}

/// A colour mixed with white.
fn tint(c: Rgb, k: f32) -> Rgb {
    [c[0] + (1.0 - c[0]) * k, c[1] + (1.0 - c[1]) * k, c[2] + (1.0 - c[2]) * k]
}

/// A weight of lettering.
#[derive(Clone, Copy, PartialEq)]
enum Wt {
    Reg,
    Bold,
    /// The bold face thickened: for headlines and labels.
    Black,
}

/// How wide words are at a size, a weight and a letter spacing (thousandths of the size).
fn width(words: &str, size: f32, wt: Wt, track: f32) -> f32 {
    let style = Style { font_size: 64.0, bold: wt != Wt::Reg, ..Style::default() };
    let n = words.chars().count().saturating_sub(1) as f32;
    text::layout(words, &style, None).width * size / 64.0 + n * track / 1000.0 * size + if wt == Wt::Black { size * 0.03 } else { 0.0 }
}

/// Words made safe for a graphic: one line, not too long.
fn line(words: &str, most: usize) -> String {
    let s: String = words.split_whitespace().collect::<Vec<_>>().join(" ");
    s.chars().take(most).collect()
}

/// Words broken into lines no wider than `max`, at most `lines` of them.
fn wrap(words: &str, size: f32, wt: Wt, max: f32, lines: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in words.split_whitespace().take(200) {
        match out.last_mut() {
            Some(l) if width(&format!("{l} {word}"), size, wt, 0.0) <= max => {
                l.push(' ');
                l.push_str(word);
            }
            _ => out.push(word.to_string()),
        }
    }
    out.truncate(lines.max(1));
    out
}

// ---- shape items ----

fn rc(r: Rect, radius: f32) -> Value {
    json!({"ty": "rc", "p": fixed(&[r.x + r.w / 2.0, r.y + r.h / 2.0]), "s": fixed(&[r.w, r.h]), "r": fixed(&[radius])})
}

fn poly(pts: &[P]) -> Value {
    json!({"ty": "sh", "ks": {"a": 0, "k": outline(pts, true)}})
}

fn el(cx: f32, cy: f32, w: f32, h: f32) -> Value {
    json!({"ty": "el", "p": fixed(&[cx, cy]), "s": fixed(&[w, h])})
}

fn fl(c: Rgb, alpha: f32) -> Value {
    json!({"ty": "fl", "c": fixed(&c), "o": fixed(&[alpha * 100.0])})
}

/// A fill shaded from `top` at `y0` to `bottom` at `y1`.
fn gv(y0: f32, y1: f32, top: Rgb, bottom: Rgb, alpha: f32) -> Value {
    let k = [0.0, top[0], top[1], top[2], 1.0, bottom[0], bottom[1], bottom[2]];
    json!({"ty": "gf", "t": 1, "o": fixed(&[alpha * 100.0]), "s": fixed(&[0.0, y0]), "e": fixed(&[0.0, y1]), "g": {"p": 2, "k": fixed(&k)}})
}

fn st(c: Rgb, alpha: f32, width: f32) -> Value {
    json!({"ty": "st", "c": fixed(&c), "o": fixed(&[alpha * 100.0]), "w": fixed(&[width]), "lc": 2, "lj": 2})
}

fn tr() -> Value {
    json!({"ty": "tr", "p": fixed(&[0.0, 0.0]), "a": fixed(&[0.0, 0.0]), "s": fixed(&[100.0, 100.0]), "r": fixed(&[0.0]), "o": fixed(&[100.0])})
}

/// Shape items as a group.
fn gr(mut items: Vec<Value>) -> Value {
    items.push(tr());
    json!({"ty": "gr", "it": items})
}

/// A dark plate's shading, and an accent plate's.
fn dark(r: Rect, alpha: f32) -> Value {
    gv(r.y, r.y + r.h, DARK2, DARK, alpha)
}

fn glow(r: Rect, a: Rgb) -> Value {
    gv(r.y, r.y + r.h, tint(a, 0.14), shade(a, 0.74), 1.0)
}

/// Which way a plate grows on.
#[derive(Clone, Copy)]
enum Dir {
    Right,
    Down,
    Up,
    /// From its middle to both sides.
    Apart,
}

/// The pieces of a graphic, top first, and the times they all share.
struct Build {
    layers: Vec<Value>,
    total: f32,
}

impl Build {
    fn new(total: f32) -> Build {
        Build { layers: Vec::new(), total: total.max(OUT * 2.0 + 2.0) }
    }

    /// Where the going starts.
    fn out(&self) -> f32 {
        self.total - OUT
    }

    fn push(&mut self, mut layer: Value) {
        if let Some(o) = layer.as_object_mut() {
            o.insert("ind".into(), json!(self.layers.len() + 1));
            if !o.contains_key("ip") {
                o.insert("ip".into(), json!(0));
                o.insert("op".into(), json!(self.total));
            }
        }
        self.layers.push(layer);
    }

    /// `off` until `from`, `on` by `to`, and back to `off` over the going (a little sooner for
    /// what came on later, so it goes as it came).
    fn on_off(&self, from: f32, to: f32, off: Vec<f32>, on: Vec<f32>) -> Value {
        let lead = (from * 0.4).min(OUT * 0.5);
        keys(&[
            (0.0, off.clone(), E::Lin),
            (from.max(0.02), off.clone(), E::Out),
            (to, on.clone(), E::Lin),
            (self.out() - lead, on, E::In),
            (self.total - lead, off, E::Lin),
        ])
    }

    /// Shape items as a layer.
    fn shapes(&mut self, name: &str, items: Vec<Value>, ks: Value) {
        self.push(json!({"ty": 4, "nm": name, "ks": ks, "shapes": items}));
    }

    /// A layer that fades on and off.
    fn faded(&mut self, name: &str, items: Vec<Value>, from: f32, to: f32) {
        let ks = json!({"o": self.on_off(from, to, vec![0.0], vec![100.0])});
        self.shapes(name, items, ks);
    }

    /// A layer that pops on about a point (a little past its size and back) and shrinks away.
    fn popped(&mut self, name: &str, items: Vec<Value>, about: P, from: f32) {
        let s = keys(&[
            (0.0, vec![0.0, 0.0], E::Lin),
            (from.max(0.02), vec![0.0, 0.0], E::Back),
            (from + 12.0, vec![100.0, 100.0], E::Lin),
            (self.out(), vec![100.0, 100.0], E::In),
            (self.total, vec![0.0, 0.0], E::Lin),
        ]);
        self.shapes(name, items, json!({"a": fixed(&about), "p": fixed(&about), "s": s}));
    }

    /// A rounded plate that grows on from an edge (or from its middle).
    fn plate(&mut self, r: Rect, radius: f32, fill: Value, from: f32, to: f32, dir: Dir) {
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let (p0, s0) = match dir {
            Dir::Right => (vec![r.x, cy], vec![0.0, r.h]),
            Dir::Down => (vec![cx, r.y], vec![r.w, 0.0]),
            Dir::Up => (vec![cx, r.y + r.h], vec![r.w, 0.0]),
            Dir::Apart => (vec![cx, cy], vec![0.0, r.h]),
        };
        let shape =
            json!({"ty": "rc", "p": self.on_off(from, to, p0, vec![cx, cy]), "s": self.on_off(from, to, s0, vec![r.w, r.h]), "r": fixed(&[radius])});
        self.shapes("plate", vec![shape, fill], json!({}));
    }

    /// A four-cornered plate (top left, top right, bottom right, bottom left) that grows on from
    /// its left edge, its cut right end leading.
    fn slant(&mut self, c: [P; 4], fill: Value, from: f32, to: f32) {
        let shut = vec![c[0], c[0], c[3], c[3]];
        let lead = (from * 0.4).min(OUT * 0.5);
        let path = outlines(&[
            (0.0, shut.clone(), E::Lin),
            (from.max(0.02), shut.clone(), E::Out),
            (to, c.to_vec(), E::Lin),
            (self.out() - lead, c.to_vec(), E::In),
            (self.total - lead, shut, E::Lin),
        ]);
        self.shapes("plate", vec![path, fill], json!({}));
    }

    /// A soft shadow under a plate (a few faint boxes, each a little larger).
    fn shadow(&mut self, r: Rect, radius: f32, from: f32) {
        let mut items = Vec::new();
        for (grow, alpha) in [(3.0, 0.10), (8.0, 0.07), (15.0, 0.05), (24.0, 0.035), (36.0, 0.02)] {
            items.push(gr(vec![rc(Rect::new(r.x - grow, r.y - grow + 10.0, r.w + grow * 2.0, r.h + grow * 2.0), radius + grow), fl(BLACK, alpha)]));
        }
        self.faded("shadow", items, from, from + 10.0);
    }

    /// A shine crossing a plate once, at `at`: a slanted light stripe seen only on the plate
    /// (`shape`: the plate's outline).
    fn shine(&mut self, shape: Value, r: Rect, at: f32) {
        self.push(json!({"ty": 4, "nm": "on the plate", "td": 1, "ks": {}, "shapes": [shape, fl(WHITE, 1.0)]}));
        let stripe = poly(&[[40.0, r.y], [110.0, r.y], [70.0, r.y + r.h], [0.0, r.y + r.h]]);
        let p =
            keys(&[(0.0, vec![r.x - 140.0, 0.0], E::Lin), (at, vec![r.x - 140.0, 0.0], E::Lin), (at + 18.0, vec![r.x + r.w + 30.0, 0.0], E::Lin)]);
        self.push(json!({"ty": 4, "nm": "shine", "tt": 1, "ks": {"p": p}, "shapes": [stripe, fl(WHITE, 0.30)]}));
    }

    /// Words that fade on, rising a little, after `from`. `just`: 0 from their left, 2 about
    /// their middle. `x`, `y`: where their first line's baseline starts (or its middle).
    #[allow(clippy::too_many_arguments)]
    fn words(&mut self, name: &str, words: &str, size: f32, wt: Wt, colour: Rgb, x: f32, y: f32, just: u8, track: f32, from: f32) {
        if words.trim().is_empty() {
            return;
        }
        let ks = json!({"p": self.on_off(from, from + 11.0, vec![x, y + size * 0.3], vec![x, y]), "o": self.on_off(from, from + 8.0, vec![0.0], vec![100.0])});
        let face = match wt {
            Wt::Reg => "Sans",
            Wt::Bold => "Sans-Bold",
            Wt::Black => "Sans-Black",
        };
        let mut doc = json!({"s": size, "f": face, "t": words, "j": just, "fc": colour, "lh": size * 1.24, "tr": track});
        if wt == Wt::Black
            && let Some(o) = doc.as_object_mut()
        {
            o.insert("sw".into(), json!(size * 0.03));
            o.insert("sc".into(), json!(colour));
        }
        self.push(json!({"ty": 5, "nm": name, "ks": ks, "t": {"d": {"k": [{"s": doc, "t": 0}]}}}));
    }

    /// Words with a soft dark copy behind them, to be read over any picture.
    #[allow(clippy::too_many_arguments)]
    fn lit(&mut self, name: &str, words: &str, size: f32, wt: Wt, colour: Rgb, x: f32, y: f32, just: u8, from: f32) {
        if words.trim().is_empty() {
            return;
        }
        self.words(name, words, size, wt, colour, x, y, just, 0.0, from);
        let d = (size * 0.03).max(1.5);
        for (off, alpha) in [(d, 0.42), (d * 2.6, 0.16)] {
            let ks = json!({"p": self.on_off(from, from + 11.0, vec![x + off * 0.5, y + size * 0.3 + off], vec![x + off * 0.5, y + off]), "o": self.on_off(from, from + 8.0, vec![0.0], vec![alpha * 100.0])});
            let doc = json!({"s": size, "f": if wt == Wt::Reg { "Sans" } else { "Sans-Bold" }, "t": words, "j": just, "fc": BLACK, "lh": size * 1.24, "sw": d * 1.4, "sc": BLACK});
            self.push(json!({"ty": 5, "nm": "behind", "ks": ks, "t": {"d": {"k": [{"s": doc, "t": 0}]}}}));
        }
    }

    /// A line through points that draws itself on, and off again.
    fn drawn(&mut self, pts: &[P], strokes: Vec<Value>, from: f32, to: f32) {
        self.drawn_shape(json!({"ty": "sh", "ks": {"a": 0, "k": outline(pts, false)}}), strokes, from, to);
    }

    fn drawn_shape(&mut self, shape: Value, strokes: Vec<Value>, from: f32, to: f32) {
        let mut items = vec![shape, json!({"ty": "tm", "s": fixed(&[0.0]), "e": self.on_off(from, to, vec![0.0], vec![100.0]), "o": fixed(&[0.0])})];
        items.extend(strokes);
        self.shapes("line", items, json!({}));
    }

    /// Full-screen pieces crossing the picture: from `from` (an offset), on it at the middle of
    /// the graphic, away to `to`.
    fn crossing(&mut self, items: Vec<Value>, from: P, to: P, start: f32, end: f32) {
        let (mid, hold) = (self.total / 2.0, self.total * 0.05);
        let p = keys(&[
            (start, from.to_vec(), E::Out),
            (mid - hold, vec![0.0, 0.0], E::Lin),
            (mid + hold, vec![0.0, 0.0], E::In),
            (end, to.to_vec(), E::Lin),
        ]);
        self.shapes("piece", items, json!({"p": p}));
    }

    fn done(self) -> Option<Animation> {
        Animation::from_value(
            json!({"v": "5.7.4", "nm": "ScreenGrabCraft", "fr": FPS, "ip": 0, "op": self.total, "w": W, "h": H, "layers": self.layers}),
        )
        .ok()
    }
}

/// A stock graphic: its animation; the box it takes up at 1920 × 1080 (for choosing and moving
/// it); the point of it that the item's place says where to put (an item placed at the middle
/// of the picture has that point there); and whether its size was built in (`size`), so it
/// isn't grown again when it's placed.
pub struct Graphic {
    pub animation: Animation,
    pub bounds: Rect,
    pub anchor: (f32, f32),
    pub sized: bool,
}

/// Whether a style is one of the graphics built here (every style is, now).
pub fn is_stock(_style: TextStyle) -> bool {
    true
}

/// Whether it's a stinger: a transition that covers the whole picture half-way through.
pub fn is_stinger(style: TextStyle) -> bool {
    use TextStyle::*;
    matches!(
        style,
        StingerBars | StingerIris | StingerSlices | StingerPanel | StingerDiagonal | StingerChevron | StingerClock | StingerShutter | StingerBlinds
    )
}

/// Whether a style has words to change.
pub fn has_words(style: TextStyle) -> bool {
    !is_stinger(style) && !matches!(style, TextStyle::Highlight | TextStyle::Spotlight | TextStyle::Circle)
}

/// Whether it has a second line.
pub fn has_subtitle(style: TextStyle) -> bool {
    use TextStyle::*;
    has_words(style) && !matches!(style, Title | TitleBox | Banner | Callout | Note | Leader | Keys)
}

/// The corners of a plate `r` whose right end is cut on a slant (`lean` wider at the top).
fn slanted(r: Rect, lean: f32) -> [P; 4] {
    [[r.x, r.y], [r.x + r.w + lean, r.y], [r.x + r.w, r.y + r.h], [r.x, r.y + r.h]]
}

/// Builds a stock graphic `len_us` long with these words and this colour. `size` is the item's
/// size (for the overlays whose size is built in: the highlight, the circle, the spotlight).
pub fn build(style: TextStyle, title: &str, subtitle: &str, accent: [u8; 3], len_us: u64, size: f32) -> Option<Graphic> {
    let total = (len_us.min(3_600_000_000) as f32 / 1e6 * FPS).max(OUT * 2.0 + 2.0);
    let a = rgb(accent);
    let (title, sub) = (line(title, 160), line(subtitle, 400));
    let has_sub = !sub.is_empty();
    let size = if size.is_finite() { size.clamp(0.05, 1.0) } else { 0.3 };
    let mut b = Build::new(total);
    let r = Rect::new;
    let middle = (W / 2.0, H / 2.0);
    // Where the old, simpler lower thirds were put by default: these keep that place.
    let lower = (576.0, 886.0);
    let (mut anchor, mut sized) = (middle, false);
    let bounds = match style {
        // ---- lower thirds ----
        TextStyle::NewsBreaking => {
            let (x, y) = (110.0, 766.0);
            let tw = width("BREAKING NEWS", 30.0, Wt::Black, 70.0) + 64.0;
            let hw = (width(&title, 60.0, Wt::Black, 0.0) + 130.0).clamp(760.0, 1700.0);
            let sw = if has_sub { (width(&sub, 32.0, Wt::Reg, 0.0) + 120.0).clamp(hw * 0.55, 1700.0) } else { 0.0 };
            let (tab, head, strip) = (r(x, y, tw, 54.0), r(x, y + 54.0, hw, 108.0), r(x, y + 162.0, sw, 58.0));
            b.words("label", "BREAKING NEWS", 30.0, Wt::Black, WHITE, x + 30.0, y + 38.0, 0, 70.0, 8.0);
            b.words("title", &title, 60.0, Wt::Black, INK, x + 44.0, y + 127.0, 0, 0.0, 14.0);
            b.words("subtitle", &sub, 32.0, Wt::Reg, WHITE, x + 44.0, y + 202.0, 0, 0.0, 18.0);
            b.shine(poly(&slanted(tab, 26.0)), tab, 22.0);
            b.shine(poly(&slanted(head, 40.0)), head, 30.0);
            b.slant(slanted(tab, 26.0), glow(tab, a), 0.0, 10.0);
            b.plate(r(x, y + 54.0, 14.0, if has_sub { 166.0 } else { 108.0 }), 0.0, fl(a, 1.0), 2.0, 12.0, Dir::Down);
            b.slant(slanted(head, 40.0), gv(head.y, head.y + head.h, WHITE, OFF, 1.0), 4.0, 18.0);
            if has_sub {
                b.slant(slanted(strip, 22.0), dark(strip, 0.96), 8.0, 22.0);
            }
            if has_sub {
                b.shadow(r(x, y + 162.0, sw - 6.0, 52.0), 0.0, 10.0);
            }
            b.shadow(r(x, y + 54.0, hw - 6.0, 102.0), 0.0, 8.0);
            r(x, y, hw.max(sw) + 40.0, if has_sub { 220.0 } else { 162.0 })
        }
        TextStyle::NewsHeadline => {
            let (y, h) = (866.0, 154.0);
            let band = r(0.0, y, W, h);
            b.words("title", &title, 62.0, Wt::Black, WHITE, 214.0, y + 74.0, 0, 0.0, 12.0);
            b.words("subtitle", &sub, 34.0, Wt::Reg, SOFT, 214.0, y + 126.0, 0, 0.0, 16.0);
            b.drawn(&[[66.0, y + 48.0], [98.0, y + 77.0], [66.0, y + 106.0]], vec![st(WHITE, 1.0, 11.0)], 12.0, 22.0);
            b.slant(slanted(r(0.0, y, 146.0, h), 44.0), glow(band, a), 4.0, 14.0);
            b.plate(r(0.0, y - 6.0, W, 6.0), 0.0, fl(a, 1.0), 0.0, 20.0, Dir::Right);
            b.plate(band, 0.0, dark(band, 0.96), 0.0, 12.0, Dir::Up);
            r(0.0, y - 6.0, W, h + 6.0)
        }
        TextStyle::NewsName => {
            let (x, y) = (110.0, 836.0);
            let w = (width(&title, 48.0, Wt::Black, 0.0).max(width(&sub, 30.0, Wt::Reg, 0.0)) + 124.0).clamp(360.0, 1500.0);
            let card = r(x, y, w, if has_sub { 130.0 } else { 92.0 });
            b.words("title", &title, 48.0, Wt::Black, WHITE, x + 60.0, y + 62.0, 0, 0.0, 10.0);
            b.words("subtitle", &sub, 30.0, Wt::Reg, tint(a, 0.55), x + 60.0, y + 106.0, 0, 0.0, 14.0);
            b.plate(r(x + 26.0, y + 24.0, 10.0, card.h - 48.0), 5.0, fl(a, 1.0), 6.0, 16.0, Dir::Down);
            b.plate(card, 20.0, dark(card, 0.95), 0.0, 14.0, Dir::Right);
            b.shadow(card, 20.0, 6.0);
            card
        }
        TextStyle::LowerBar => {
            anchor = lower;
            let (x, y) = (110.0, 836.0);
            let nw = (width(&title, 46.0, Wt::Black, 0.0) + 110.0).clamp(300.0, 1500.0);
            let rw = if has_sub { (width(&sub, 28.0, Wt::Bold, 0.0) + 100.0).clamp(240.0, 1500.0) } else { 0.0 };
            let name = [[x + 30.0, y], [x + nw + 30.0, y], [x + nw, y + 76.0], [x, y + 76.0]];
            let role = [[x + 30.0, y + 76.0], [x + rw + 52.0, y + 76.0], [x + rw + 30.0, y + 130.0], [x + 8.0, y + 130.0]];
            b.words("title", &title, 46.0, Wt::Black, WHITE, x + 56.0, y + 54.0, 0, 0.0, 8.0);
            b.words("subtitle", &sub, 28.0, Wt::Bold, INK, x + 60.0, y + 113.0, 0, 0.0, 12.0);
            b.shine(poly(&name), r(x, y, nw + 30.0, 76.0), 20.0);
            b.slant(name, glow(r(x, y, nw, 76.0), a), 0.0, 12.0);
            if has_sub {
                b.slant(role, gv(y + 76.0, y + 130.0, WHITE, OFF, 1.0), 4.0, 16.0);
            }
            b.shadow(r(x + 14.0, y + 4.0, nw.max(rw), if has_sub { 124.0 } else { 70.0 }), 0.0, 6.0);
            r(x, y, nw.max(rw + 22.0) + 30.0, if has_sub { 130.0 } else { 76.0 })
        }
        TextStyle::LowerLine => {
            anchor = lower;
            let (x, y) = (110.0, 832.0);
            let w = width(&title, 56.0, Wt::Black, 0.0).max(width(&sub, 32.0, Wt::Reg, 0.0)).clamp(200.0, 1600.0);
            let h = if has_sub { 116.0 } else { 66.0 };
            b.lit("title", &title, 56.0, Wt::Black, WHITE, x + 34.0, y + 52.0, 0, 6.0);
            b.lit("subtitle", &sub, 32.0, Wt::Reg, WHITE, x + 34.0, y + 102.0, 0, 10.0);
            b.plate(r(x, y, 8.0, h), 4.0, fl(a, 1.0), 0.0, 10.0, Dir::Down);
            r(x, y, w + 40.0, h)
        }
        TextStyle::LowerTag => {
            anchor = lower;
            let (x, y, h) = (110.0, 836.0, 108.0);
            let w = (width(&title, 40.0, Wt::Bold, 0.0).max(width(&sub, 26.0, Wt::Reg, 0.0)) + 176.0).clamp(340.0, 1500.0);
            let pill = r(x, y, w, h);
            let initials: String = title.split_whitespace().take(2).filter_map(|w| w.chars().next()).flat_map(char::to_uppercase).collect();
            let (cx, cy) = (x + 54.0, y + 54.0);
            b.words("initials", &initials, 36.0, Wt::Black, WHITE, cx, cy + 13.0, 2, 20.0, 8.0);
            b.words("title", &title, 40.0, Wt::Bold, WHITE, x + 122.0, y + if has_sub { 50.0 } else { 67.0 }, 0, 0.0, 10.0);
            b.words("subtitle", &sub, 26.0, Wt::Reg, SOFT, x + 122.0, y + 86.0, 0, 0.0, 14.0);
            b.popped("ring", vec![el(cx, cy, 88.0, 88.0), st(WHITE, 0.35, 3.0)], [cx, cy], 4.0);
            b.popped("face", vec![el(cx, cy, 80.0, 80.0), gv(cy - 40.0, cy + 40.0, tint(a, 0.2), shade(a, 0.72), 1.0)], [cx, cy], 2.0);
            b.plate(pill, 54.0, dark(pill, 0.95), 0.0, 14.0, Dir::Right);
            b.shadow(pill, 54.0, 6.0);
            pill
        }
        // ---- news and broadcast ----
        TextStyle::NewsLive => {
            let (x, y) = (110.0, 70.0);
            let place = if has_sub { format!("{title}   {sub}") } else { title.clone() };
            let lw = if place.is_empty() { 0.0 } else { width(&place, 30.0, Wt::Bold, 0.0) + 64.0 };
            let (pill, side) = (r(x, y, 170.0, 58.0), r(x + 140.0, y, lw + 30.0, 58.0));
            b.words("label", "LIVE", 30.0, Wt::Black, WHITE, x + 60.0, y + 40.0, 0, 90.0, 4.0);
            b.words("title", &place, 30.0, Wt::Bold, WHITE, x + 196.0, y + 40.0, 0, 0.0, 10.0);
            // The dot beats once a second, a ring going out from it.
            let (mut beat, mut ring, mut fade) = (vec![(0.0, vec![100.0], E::Lin)], Vec::new(), Vec::new());
            let mut t = 0.0;
            while t < total && ring.len() < 600 {
                ring.push((t, vec![16.0, 16.0], E::Out));
                ring.push((t + 22.0, vec![46.0, 46.0], E::Lin));
                fade.push((t, vec![80.0], E::Out));
                fade.push((t + 22.0, vec![0.0], E::Lin));
                beat.push((t + 15.0, vec![45.0], E::Lin));
                beat.push((t + 29.0, vec![100.0], E::Lin));
                t += 30.0;
            }
            let (cx, cy) = (x + 34.0, y + 29.0);
            let ring_shape = json!({"ty": "el", "p": fixed(&[cx, cy]), "s": keys(&ring)});
            let ring_line = json!({"ty": "st", "c": fixed(&WHITE), "o": keys(&fade), "w": fixed(&[3.0])});
            b.faded("ring", vec![ring_shape, ring_line], 6.0, 12.0);
            let dot_fill = json!({"ty": "fl", "c": fixed(&WHITE), "o": keys(&beat)});
            b.faded("dot", vec![el(cx, cy, 16.0, 16.0), dot_fill], 4.0, 10.0);
            b.plate(pill, 29.0, glow(pill, a), 0.0, 9.0, Dir::Right);
            if lw > 0.0 {
                b.plate(side, 29.0, dark(side, 0.94), 4.0, 16.0, Dir::Right);
            }
            b.shadow(r(x, y, 170.0 + lw, 58.0), 29.0, 6.0);
            r(x, y, 170.0 + lw, 58.0)
        }
        TextStyle::NewsTicker => {
            let (y, h) = (996.0, 70.0);
            let label = title.to_uppercase();
            let lbw = (width(&label, 30.0, Wt::Black, 60.0) + 70.0).clamp(140.0, 620.0);
            let band = r(0.0, y, W, h);
            b.words("title", &label, 30.0, Wt::Black, WHITE, 30.0, y + 46.0, 0, 60.0, 6.0);
            b.slant(slanted(r(0.0, y, lbw, h), 34.0), glow(band, a), 2.0, 12.0);
            if has_sub {
                // The words pass at a steady pace, again and again, seen only right of the label.
                let crawl: String = std::iter::repeat_n(sub.as_str(), 8).collect::<Vec<_>>().join("     •     ").chars().take(2000).collect();
                b.push(json!({"ty": 4, "nm": "window", "td": 1, "ks": {}, "shapes": [rc(r(lbw, y, W - lbw, h), 0.0), fl(WHITE, 1.0)]}));
                let p = keys(&[(0.0, vec![lbw + 70.0, y + 46.0], E::Lin), (total, vec![lbw + 70.0 - 150.0 * total / FPS, y + 46.0], E::Lin)]);
                let ks = json!({"p": p, "o": b.on_off(6.0, 14.0, vec![0.0], vec![100.0])});
                let doc = json!({"s": 30.0, "f": "Sans", "t": crawl, "j": 0, "fc": WHITE});
                b.push(json!({"ty": 5, "nm": "subtitle", "tt": 1, "ks": ks, "t": {"d": {"k": [{"s": doc, "t": 0}]}}}));
            }
            b.plate(r(0.0, y - 5.0, W, 5.0), 0.0, fl(a, 1.0), 0.0, 14.0, Dir::Right);
            b.plate(band, 0.0, dark(band, 0.96), 0.0, 10.0, Dir::Up);
            r(0.0, y - 5.0, W, h + 5.0)
        }
        TextStyle::NewsLocation => {
            let (x, y) = (110.0, 70.0);
            let bw = (width(&title, 38.0, Wt::Bold, 0.0).max(width(&sub, 26.0, Wt::Reg, 0.0)) + 112.0).clamp(240.0, 1400.0);
            let card = r(x, y, bw, if has_sub { 106.0 } else { 68.0 });
            let (cx, cy) = (x + 38.0, y + 34.0);
            b.words("title", &title, 38.0, Wt::Bold, WHITE, x + 74.0, y + 47.0, 0, 0.0, 6.0);
            b.words("subtitle", &sub, 26.0, Wt::Reg, SOFT, x + 74.0, y + 86.0, 0, 0.0, 10.0);
            b.popped("ring", vec![el(cx, cy, 32.0, 32.0), st(WHITE, 0.6, 3.0)], [cx, cy], 6.0);
            b.popped("pin", vec![el(cx, cy, 18.0, 18.0), fl(a, 1.0)], [cx, cy], 4.0);
            b.plate(card, 16.0, dark(card, 0.94), 0.0, 12.0, Dir::Right);
            b.shadow(card, 16.0, 6.0);
            card
        }
        TextStyle::NewsQuote => {
            let said = wrap(&title, 50.0, Wt::Reg, 930.0, 4);
            let n = said.len().max(1) as f32;
            let ch = 150.0 + n * 62.0 + if has_sub { 90.0 } else { 20.0 };
            let top = (H - ch) / 2.0;
            let card = r(360.0, top, 1200.0, ch);
            b.words("mark", "“", 190.0, Wt::Black, a, 410.0, top + 190.0, 0, 0.0, 4.0);
            b.words("title", &said.join("\n"), 50.0, Wt::Reg, WHITE, 540.0, top + 132.0, 0, 0.0, 8.0);
            b.words("subtitle", &sub, 30.0, Wt::Bold, tint(a, 0.5), 616.0, top + ch - 50.0, 0, 0.0, 14.0);
            if has_sub {
                b.plate(r(540.0, top + ch - 63.0, 56.0, 5.0), 2.5, fl(a, 1.0), 12.0, 20.0, Dir::Right);
            }
            b.plate(r(360.0, top + 24.0, 10.0, ch - 48.0), 5.0, fl(a, 1.0), 2.0, 14.0, Dir::Down);
            b.plate(card, 24.0, dark(card, 0.95), 0.0, 14.0, Dir::Right);
            b.shadow(card, 24.0, 6.0);
            card
        }
        TextStyle::NewsStat => {
            let bw = (width(&title, 150.0, Wt::Black, 0.0).max(width(&sub, 34.0, Wt::Bold, 0.0)) + 120.0).clamp(380.0, 900.0);
            let x = W - 110.0 - bw;
            let card = r(x, 290.0, bw, 320.0);
            b.words("title", &title, 150.0, Wt::Black, WHITE, x + bw / 2.0, 472.0, 2, 0.0, 6.0);
            b.words("subtitle", &sub, 34.0, Wt::Bold, WHITE, x + bw / 2.0, 562.0, 2, 0.0, 12.0);
            b.plate(r(x + bw / 2.0 - 44.0, 500.0, 88.0, 5.0), 2.5, fl(WHITE, 0.85), 8.0, 18.0, Dir::Apart);
            b.shine(rc(card, 24.0), card, 24.0);
            b.plate(card, 24.0, glow(card, a), 0.0, 12.0, Dir::Down);
            b.shadow(card, 24.0, 6.0);
            card
        }
        // ---- titles ----
        TextStyle::TitleBroadcast => {
            let tw = width(&title, 104.0, Wt::Black, 0.0).max(width(&sub, 44.0, Wt::Reg, 0.0)).clamp(300.0, 1720.0);
            let x = W / 2.0;
            b.lit("title", &title, 104.0, Wt::Black, WHITE, x, 524.0, 2, 4.0);
            b.lit("subtitle", &sub, 44.0, Wt::Reg, WHITE, x, 634.0, 2, 10.0);
            b.plate(r(x - tw / 2.0, 556.0, tw, 8.0), 4.0, fl(a, 1.0), 6.0, 22.0, Dir::Apart);
            r(x - tw / 2.0 - 30.0, 424.0, tw + 60.0, if has_sub { 236.0 } else { 150.0 })
        }
        TextStyle::Title => {
            anchor = (960.0, 486.0);
            let tw = width(&title, 112.0, Wt::Black, 0.0).clamp(200.0, 1760.0);
            b.lit("title", &title, 112.0, Wt::Black, a, 960.0, 524.0, 2, 2.0);
            r(960.0 - tw / 2.0 - 20.0, 412.0, tw + 40.0, 150.0)
        }
        TextStyle::TitleBox => {
            anchor = (960.0, 486.0);
            let w = (width(&title, 84.0, Wt::Black, 0.0) + 150.0).clamp(360.0, 1760.0);
            let card = r(960.0 - w / 2.0, 394.0, w, 184.0);
            b.words("title", &title, 84.0, Wt::Black, WHITE, 960.0, 516.0, 2, 0.0, 8.0);
            b.shine(rc(card, 22.0), card, 22.0);
            b.popped("card", vec![rc(card, 22.0), glow(card, a)], [960.0, 486.0], 0.0);
            b.shadow(card, 22.0, 8.0);
            card
        }
        TextStyle::Banner => {
            anchor = (960.0, 86.0);
            let band = r(0.0, 36.0, W, 100.0);
            b.words("title", &title, 50.0, Wt::Black, WHITE, 960.0, 103.0, 2, 20.0, 8.0);
            b.plate(r(0.0, 136.0, W, 5.0), 0.0, fl(tint(a, 0.5), 1.0), 4.0, 18.0, Dir::Right);
            b.plate(band, 0.0, glow(band, a), 0.0, 12.0, Dir::Down);
            r(0.0, 36.0, W, 105.0)
        }
        // ---- callouts and overlays ----
        TextStyle::Callout => {
            anchor = (1190.0, 378.0);
            let said = wrap(&title, 40.0, Wt::Bold, 560.0, 3);
            let widest = said.iter().map(|l| width(l, 40.0, Wt::Bold, 0.0)).fold(0.0, f32::max);
            let (w, h) = ((widest + 92.0).clamp(220.0, 720.0), said.len().max(1) as f32 * 50.0 + 66.0);
            let card = r(1190.0 - w / 2.0, 378.0 - h / 2.0, w, h);
            let tail = [[card.x + 54.0, card.y + h - 3.0], [card.x + 118.0, card.y + h - 3.0], [card.x + 44.0, card.y + h + 46.0]];
            let paper = gv(card.y, card.y + h + 46.0, WHITE, OFF, 1.0);
            b.words("title", &said.join("\n"), 40.0, Wt::Bold, INK, card.x + 46.0, card.y + 70.0, 0, 0.0, 8.0);
            let about = [card.x + 44.0, card.y + h + 46.0];
            b.popped("edge", vec![rc(r(card.x + 24.0, card.y + h - 9.0, w - 48.0, 5.0), 2.5), fl(a, 1.0)], about, 0.0);
            b.popped("bubble", vec![gr(vec![poly(&tail), paper.clone()]), gr(vec![rc(card, 24.0), paper])], about, 0.0);
            b.shadow(card, 24.0, 8.0);
            r(card.x, card.y, w, h + 46.0)
        }
        TextStyle::Note => {
            anchor = (1440.0, 270.0);
            let said = wrap(&title, 38.0, Wt::Bold, 330.0, 5);
            let (w, h) = (420.0, (said.len().max(1) as f32 * 48.0 + 116.0).max(250.0));
            let (x, y) = (1440.0 - w / 2.0, 270.0 - h / 2.0);
            let fold = 60.0;
            let sheet = [[x, y], [x + w, y], [x + w, y + h - fold], [x + w - fold, y + h], [x, y + h]];
            b.words("title", &said.join("\n"), 38.0, Wt::Bold, INK, x + 44.0, y + 92.0, 0, 0.0, 8.0);
            let about = [x + w / 2.0, y];
            b.popped("tape", vec![rc(r(x + w / 2.0 - 74.0, y - 20.0, 148.0, 40.0), 3.0), fl(WHITE, 0.6)], about, 2.0);
            let corner = gr(vec![poly(&[[x + w - fold, y + h], [x + w, y + h - fold], [x + w - fold, y + h - fold]]), fl(shade(a, 0.74), 1.0)]);
            b.popped("sheet", vec![corner, gr(vec![poly(&sheet), gv(y, y + h, tint(a, 0.3), a, 1.0)])], about, 0.0);
            b.shadow(r(x, y, w, h - 8.0), 4.0, 8.0);
            r(x, y - 20.0, w, h + 20.0)
        }
        TextStyle::Leader => {
            let (px, py) = middle;
            let lw = (width(&title, 34.0, Wt::Bold, 0.0) + 68.0).clamp(160.0, 1000.0);
            let card = r(px + 196.0, py - 184.0, lw, 68.0);
            b.words("title", &title, 34.0, Wt::Bold, WHITE, card.x + 34.0, card.y + 46.0, 0, 0.0, 16.0);
            b.plate(r(card.x, card.y + 14.0, 8.0, 40.0), 4.0, fl(a, 1.0), 14.0, 22.0, Dir::Down);
            b.plate(card, 14.0, dark(card, 0.95), 12.0, 22.0, Dir::Right);
            let path = [[px, py], [px + 150.0, py - 150.0], [px + 196.0, py - 150.0]];
            b.drawn(&path, vec![st(WHITE, 1.0, 4.0), st(BLACK, 0.35, 9.0)], 4.0, 14.0);
            b.popped("dot", vec![el(px, py, 20.0, 20.0), fl(a, 1.0)], [px, py], 0.0);
            b.popped("ring", vec![el(px, py, 40.0, 40.0), st(WHITE, 0.95, 3.5)], [px, py], 2.0);
            b.shadow(card, 14.0, 14.0);
            r(px - 22.0, py - 184.0, 218.0 + lw, 206.0)
        }
        TextStyle::Step => {
            let (cx, cy) = middle;
            let num: String = title.chars().take(3).collect();
            let lw = if has_sub { width(&sub, 36.0, Wt::Bold, 0.0) + 100.0 } else { 0.0 };
            let side = r(cx + 10.0, cy - 38.0, lw + 30.0, 76.0);
            b.words("title", &num, 52.0, Wt::Black, WHITE, cx, cy + 19.0, 2, 0.0, 6.0);
            b.words("subtitle", &sub, 36.0, Wt::Bold, WHITE, cx + 74.0, cy + 13.0, 0, 0.0, 12.0);
            b.popped("ring", vec![el(cx, cy, 104.0, 104.0), st(WHITE, 0.95, 4.0)], [cx, cy], 2.0);
            b.popped("badge", vec![el(cx, cy, 100.0, 100.0), gv(cy - 50.0, cy + 50.0, tint(a, 0.18), shade(a, 0.72), 1.0)], [cx, cy], 0.0);
            if has_sub {
                b.plate(side, 38.0, dark(side, 0.95), 6.0, 18.0, Dir::Right);
                b.shadow(side, 38.0, 8.0);
            }
            r(cx - 52.0, cy - 52.0, 104.0 + lw, 104.0)
        }
        TextStyle::Keys => {
            let (cx, cy) = middle;
            let caps: Vec<String> = title.split('+').map(str::trim).filter(|k| !k.is_empty()).take(5).map(|k| k.chars().take(12).collect()).collect();
            let widths: Vec<f32> = caps.iter().map(|k| (width(k, 38.0, Wt::Bold, 0.0) + 60.0).max(92.0)).collect();
            let (gap, h) = (56.0, 88.0);
            let whole = (widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32).max(10.0);
            let (mut x, y) = (cx - whole / 2.0, cy - h / 2.0);
            let mut keys_drawn = Vec::new();
            for (k, (cap, w)) in caps.iter().zip(&widths).enumerate() {
                let from = 2.0 + k as f32 * 4.0;
                b.words("title", cap, 38.0, Wt::Bold, INK, x + w / 2.0, y + 56.0, 2, 0.0, from + 5.0);
                if k > 0 {
                    b.lit("plus", "+", 40.0, Wt::Bold, WHITE, x - gap / 2.0, y + 58.0, 2, from + 2.0);
                }
                let face = gr(vec![rc(r(x, y, *w, h), 14.0), gv(y, y + h, WHITE, [0.86, 0.88, 0.92], 1.0)]);
                let side = gr(vec![rc(r(x, y + 9.0, *w, h), 14.0), fl([0.58, 0.61, 0.68], 1.0)]);
                keys_drawn.push((vec![face, side], [x + w / 2.0, y + h / 2.0], from));
                x += w + gap;
            }
            let mut left = cx - whole / 2.0;
            for ((items, about, from), w) in keys_drawn.into_iter().zip(&widths) {
                b.popped("key", items, about, from);
                b.shadow(r(left + 4.0, y + 12.0, w - 8.0, h - 8.0), 14.0, from + 6.0);
                left += w + gap;
            }
            r(cx - whole / 2.0, y, whole, h + 9.0)
        }
        TextStyle::Highlight | TextStyle::Circle => {
            sized = true;
            let (cx, cy) = middle;
            let w = size * W;
            let round = style == TextStyle::Circle;
            let h = w * if round { 0.66 } else { 0.6 };
            let shape = |grow: f32| if round { el(cx, cy, w + grow, h + grow) } else { rc(r(cx - w / 2.0, cy - h / 2.0, w, h), 18.0) };
            b.drawn_shape(shape(0.0), vec![st(a, 1.0, 7.0), st(a, 0.25, 18.0)], 0.0, 16.0);
            if round {
                // A second turn, a little out of true, as a pen goes round twice.
                let ks = json!({"a": fixed(&[cx, cy]), "p": fixed(&[cx, cy]), "r": fixed(&[-7.0])});
                let turn = json!({"ty": "tm", "s": fixed(&[0.0]), "e": b.on_off(8.0, 24.0, vec![0.0], vec![100.0]), "o": fixed(&[0.0])});
                b.shapes("again", vec![shape(14.0), turn, st(a, 0.9, 5.0)], ks);
            } else {
                b.faded("wash", vec![shape(0.0), fl(a, 0.10)], 6.0, 16.0);
            }
            r(cx - w / 2.0, cy - h / 2.0, w, h)
        }
        TextStyle::Spotlight => {
            sized = true;
            let (cx, cy) = middle;
            let radius = size * W * 0.62;
            // Dark all round, clear in the middle, soft between.
            let k = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.56, 0.0, 0.84, 0.66, 1.0, 0.72];
            let fill = json!({"ty": "gf", "t": 2, "o": fixed(&[100.0]), "s": fixed(&[cx, cy]), "e": fixed(&[cx + radius, cy]), "g": {"p": 2, "k": fixed(&k)}});
            b.faded("dark", vec![rc(r(-W, -H, W * 3.0, H * 3.0), 0.0), fill], 0.0, 12.0);
            r(cx - radius * 0.6, cy - radius * 0.6, radius * 1.2, radius * 1.2)
        }
        // ---- stingers: all of the picture covered half-way through ----
        TextStyle::StingerDiagonal | TextStyle::StingerPanel => {
            let lean = 420.0;
            let slab = |x0: f32, x1: f32| poly(&[[x0 + lean, 0.0], [x1 + lean, 0.0], [x1, H], [x0, H]]);
            let (x0, x1) = (-lean - 60.0, W + 60.0);
            let far = W + lean + 420.0;
            let colour = gv(0.0, H, tint(a, 0.16), shade(a, 0.7), 1.0);
            if style == TextStyle::StingerDiagonal {
                let items = vec![
                    gr(vec![slab(x1 + 34.0, x1 + 62.0), fl(WHITE, 0.95)]),
                    gr(vec![slab(x1 + 92.0, x1 + 132.0), fl(tint(a, 0.4), 1.0)]),
                    gr(vec![slab(x0 - 62.0, x0 - 34.0), fl(WHITE, 0.95)]),
                    gr(vec![slab(x0 - 132.0, x0 - 92.0), fl(shade(a, 0.6), 1.0)]),
                    gr(vec![slab(x0, x1), colour]),
                ];
                b.crossing(items, [-far, 0.0], [far, 0.0], 0.0, b.total);
            } else {
                // Two panels, the dark one leading, a bright line between them.
                let lag = b.total * 0.07;
                b.crossing(
                    vec![gr(vec![slab(x1 - 4.0, x1 + 16.0), fl(WHITE, 1.0)]), gr(vec![slab(x0, x1), colour])],
                    [-far, 0.0],
                    [far, 0.0],
                    lag,
                    b.total,
                );
                b.crossing(vec![slab(x0, x1), gv(0.0, H, DARK2, DARK, 1.0)], [-far, 0.0], [far, 0.0], 0.0, b.total - lag);
            }
            r(0.0, 0.0, W, H)
        }
        TextStyle::StingerChevron => {
            let t = 300.0;
            let arrow = |x0: f32, x1: f32| poly(&[[x0, 0.0], [x1, 0.0], [x1 + t, H / 2.0], [x1, H], [x0, H], [x0 + t, H / 2.0]]);
            let (x0, x1) = (-t - 60.0, W + 60.0);
            let far = W + t + 460.0;
            let items = vec![
                gr(vec![arrow(x1 + 40.0, x1 + 76.0), fl(WHITE, 0.95)]),
                gr(vec![arrow(x1 + 116.0, x1 + 176.0), fl(tint(a, 0.4), 1.0)]),
                gr(vec![arrow(x0 - 76.0, x0 - 40.0), fl(WHITE, 0.95)]),
                gr(vec![arrow(x0 - 176.0, x0 - 116.0), fl(shade(a, 0.6), 1.0)]),
                gr(vec![arrow(x0, x1), gv(0.0, H, tint(a, 0.16), shade(a, 0.7), 1.0)]),
            ];
            b.crossing(items, [-far, 0.0], [far, 0.0], 0.0, b.total);
            r(0.0, 0.0, W, H)
        }
        TextStyle::StingerClock => {
            // A hand sweeps the colour on round the clock, then sweeps it off the same way: a
            // ring drawn so thick it has no hole, more and more of it.
            let mid = b.total / 2.0;
            let turn = json!({"ty": "tm", "s": keys(&[(mid, vec![0.0], E::Lin), (b.total, vec![100.0], E::Lin)]), "e": keys(&[(0.0, vec![0.0], E::Lin), (mid, vec![100.0], E::Lin)]), "o": fixed(&[0.0])});
            let ring = json!({"ty": "st", "c": fixed(&a), "o": fixed(&[100.0]), "w": fixed(&[1340.0]), "lc": 1, "lj": 2});
            b.shapes("sweep", vec![el(0.0, 0.0, 1340.0, 1340.0), turn, ring], json!({"p": fixed(&[W / 2.0, H / 2.0]), "r": fixed(&[-90.0])}));
            r(0.0, 0.0, W, H)
        }
        TextStyle::StingerShutter => {
            // Two halves close on the middle, a bright line on each lip, and open again.
            let half = H / 2.0;
            let top = vec![
                gr(vec![rc(r(0.0, half - 10.0, W, 10.0), 0.0), fl(a, 1.0)]),
                gr(vec![rc(r(0.0, -40.0, W, half + 42.0), 0.0), gv(0.0, half, DARK2, DARK, 1.0)]),
            ];
            let bottom = vec![
                gr(vec![rc(r(0.0, half, W, 10.0), 0.0), fl(tint(a, 0.35), 1.0)]),
                gr(vec![rc(r(0.0, half - 2.0, W, half + 42.0), 0.0), gv(half, H, DARK, DARK2, 1.0)]),
            ];
            b.crossing(top, [0.0, -half - 60.0], [0.0, -half - 60.0], 0.0, b.total);
            b.crossing(bottom, [0.0, half + 60.0], [0.0, half + 60.0], 0.0, b.total);
            r(0.0, 0.0, W, H)
        }
        TextStyle::StingerBlinds | TextStyle::StingerBars | TextStyle::StingerSlices => {
            // Strips, one after another: blinds turn shut and open where they are; bars and
            // slices cross the picture, each the other way from the one before.
            let n = if style == TextStyle::StingerSlices { 8usize } else { 7 };
            let (mid, lag) = (b.total / 2.0, b.total * 0.03);
            for k in 0..n {
                let (f, across) = (k as f32, style != TextStyle::StingerSlices);
                let strip = if across {
                    r(0.0, f * H / n as f32 - 1.0, W, H / n as f32 + 2.0)
                } else {
                    r(f * W / n as f32 - 1.0, 0.0, W / n as f32 + 2.0, H)
                };
                let even = k % 2 == 0;
                let fill = gv(strip.y, strip.y + strip.h, tint(a, if even { 0.14 } else { 0.0 }), shade(a, if even { 0.78 } else { 0.62 }), 1.0);
                let (start, end) = (f * lag, b.total - (n - 1 - k) as f32 * lag);
                if style == TextStyle::StingerBlinds {
                    let hold = b.total * 0.05;
                    let (cx, cy) = (strip.x + strip.w / 2.0, strip.y + strip.h / 2.0);
                    let p = keys(&[
                        (start, vec![cx, strip.y], E::Out),
                        (mid - hold, vec![cx, cy], E::Lin),
                        (mid + hold, vec![cx, cy], E::In),
                        (end, vec![cx, strip.y + strip.h], E::Lin),
                    ]);
                    let s = keys(&[
                        (start, vec![strip.w, 0.0], E::Out),
                        (mid - hold, vec![strip.w, strip.h], E::Lin),
                        (mid + hold, vec![strip.w, strip.h], E::In),
                        (end, vec![strip.w, 0.0], E::Lin),
                    ]);
                    b.shapes("blind", vec![json!({"ty": "rc", "p": p, "s": s, "r": fixed(&[0.0])}), fill], json!({}));
                } else {
                    let way = if even { 1.0 } else { -1.0 };
                    let off = if across { [-(W + 40.0) * way, 0.0] } else { [0.0, -(H + 40.0) * way] };
                    b.crossing(vec![rc(strip, 0.0), fill], off, [-off[0], -off[1]], start, end);
                }
            }
            r(0.0, 0.0, W, H)
        }
        TextStyle::StingerIris => {
            // A disc grows to cover the picture, a bright ring at its edge; then a hole grows in
            // the colour, uncovering it.
            let mid = b.total / 2.0;
            let disc = |from: f32, to: f32, e: E, paint: Value| {
                vec![
                    json!({"ty": "el", "p": fixed(&[W / 2.0, H / 2.0]), "s": keys(&[(from, vec![0.0, 0.0], e), (to, vec![2340.0, 2340.0], E::Lin)])}),
                    paint,
                ]
            };
            let hex = format!("#{:02x}{:02x}{:02x}", accent[0], accent[1], accent[2]);
            b.layers
                .push(json!({"ty": 4, "nm": "ring", "ind": 1, "ip": 0, "op": mid, "ks": {}, "shapes": disc(0.0, mid, E::In, st(WHITE, 0.95, 16.0))}));
            b.layers.push(json!({"ty": 4, "nm": "cover", "ind": 2, "ip": 0, "op": mid, "ks": {}, "shapes": disc(0.0, mid, E::In, fl(a, 1.0))}));
            b.layers.push(json!({"ty": 4, "nm": "lip", "ind": 3, "ip": mid, "op": b.total, "ks": {}, "shapes": disc(mid, b.total, E::Out, st(WHITE, 0.95, 16.0))}));
            b.layers.push(json!({"ty": 4, "nm": "hole", "ind": 4, "td": 1, "ip": mid, "op": b.total, "ks": {}, "shapes": disc(mid, b.total, E::Out, fl(WHITE, 1.0))}));
            b.layers.push(json!({"ty": 1, "nm": "colour", "ind": 5, "tt": 2, "ip": mid, "op": b.total, "ks": {}, "sw": W, "sh": H, "sc": hex}));
            r(0.0, 0.0, W, H)
        }
    };
    Some(Graphic { animation: b.done()?, bounds, anchor, sized })
}

/// A graphic as the library's card shows it, over `backdrop`: close up, as it is once it has
/// come on (a stinger part-way across the whole of it; a wide one from its left, so its words
/// can be read).
pub fn card(
    style: TextStyle,
    title: &str,
    subtitle: &str,
    accent: [u8; 3],
    len_us: u64,
    size: f32,
    backdrop: &screengrabcraft_model::Raster,
) -> Option<screengrabcraft_model::Raster> {
    use tiny_skia::Transform;
    let g = build(style, title, subtitle, accent, len_us, size)?;
    let mut pix = screengrabcraft_render::canvas::to_pixmap(backdrop)?;
    let (w, h) = (pix.width() as f32, pix.height() as f32);
    let secs = len_us.min(3_600_000_000) as f32 / 1e6;
    if is_stinger(style) {
        g.animation.render(&mut pix, secs * 0.32, Transform::from_scale(w / W, h / H));
    } else if style == TextStyle::Spotlight {
        g.animation.render(&mut pix, secs / 2.0, Transform::from_scale(w / W, h / H));
    } else {
        let b = g.bounds;
        // Large enough to read, even if that shows only its left end.
        let k = ((w - 20.0) / b.w.max(1.0)).min((h - 20.0) / b.h.max(1.0)).clamp(0.24, 0.62).min((h - 12.0) / b.h.max(1.0));
        let left = if b.w * k > w - 20.0 { 10.0 } else { (w - b.w * k) / 2.0 };
        let at = Transform::from_translate(-b.x, -(b.y + b.h / 2.0)).post_scale(k, k).post_translate(left, h / 2.0);
        g.animation.render(&mut pix, secs / 2.0, at);
    }
    screengrabcraft_render::canvas::from_pixmap(&pix)
}

#[cfg(test)]
const ALL: [TextStyle; 32] = {
    use TextStyle::*;
    [
        NewsBreaking,
        NewsHeadline,
        NewsName,
        LowerBar,
        LowerLine,
        LowerTag,
        NewsLive,
        NewsTicker,
        NewsLocation,
        NewsQuote,
        NewsStat,
        TitleBroadcast,
        Title,
        TitleBox,
        Banner,
        Callout,
        Note,
        Leader,
        Step,
        Keys,
        Highlight,
        Circle,
        Spotlight,
        StingerDiagonal,
        StingerPanel,
        StingerChevron,
        StingerClock,
        StingerShutter,
        StingerBlinds,
        StingerBars,
        StingerSlices,
        StingerIris,
    ]
};

#[cfg(test)]
fn sample(style: TextStyle) -> (&'static str, &'static str) {
    match style {
        TextStyle::Keys => ("Ctrl + Shift + S", ""),
        TextStyle::Step => ("2", "Choose where it goes"),
        _ => ("City Council Approves Budget", "Vote passes 7 to 2 after a long night"),
    }
}

#[cfg(test)]
mod tests {
    use tiny_skia::{Pixmap, Transform};

    use super::*;

    /// How much of the picture is drawn on at a frame.
    fn covered(g: &Graphic, frame: f32) -> f32 {
        let mut pix = Pixmap::new(480, 270).unwrap();
        g.animation.render_frame(&mut pix, frame, Transform::from_scale(0.25, 0.25));
        pix.pixels().iter().filter(|p| p.alpha() > 128).count() as f32 / (480.0 * 270.0)
    }

    #[test]
    fn every_graphic_comes_on_stays_and_goes() {
        for style in ALL {
            let (title, sub) = sample(style);
            let g = build(style, title, sub, [208, 2, 27], 5_000_000, 0.3);
            assert!(g.is_some(), "{style:?}");
            let g = g.unwrap();
            assert_eq!(g.animation.duration_us(), 5_000_000, "{style:?}");
            let (start, held, end) = (covered(&g, 0.0), covered(&g, 75.0), covered(&g, 149.9));
            assert!(start < 0.01 && end < 0.03, "{style:?}: nothing at its start or its end ({start}, {end})");
            assert!(g.bounds.w > 10.0 && g.bounds.h > 10.0, "{style:?} {:?}", g.bounds);
            if is_stinger(style) {
                assert!(held > 0.995, "{style:?} covers the picture half-way: {held}");
            } else if style == TextStyle::Spotlight {
                assert!(held > 0.3 && held < 0.99, "the spotlight darkens all but its middle: {held}");
            } else {
                assert!(held > 0.002 && held < 0.6, "{style:?} is on the picture, not over all of it: {held}");
            }
            if has_words(style) {
                assert!(!g.animation.texts().is_empty(), "{style:?} has its words");
            }
        }
    }

    #[test]
    fn a_graphic_fits_its_words_and_takes_any_length_and_any_words() {
        let short = build(TextStyle::NewsBreaking, "Fire", "", [208, 2, 27], 4_000_000, 0.3).unwrap();
        let long = build(
            TextStyle::NewsBreaking,
            "Wildfire forces thousands to leave their homes",
            "Crews work through the night",
            [208, 2, 27],
            4_000_000,
            0.3,
        )
        .unwrap();
        assert!(long.bounds.w > short.bounds.w + 300.0 && long.bounds.h > short.bounds.h, "{:?} {:?}", short.bounds, long.bounds);
        // Its words are there to change, by name, as in any Lottie file.
        let names: Vec<String> = long.animation.texts().into_iter().map(|(n, _)| n).collect();
        assert!(names.contains(&"title".to_string()) && names.contains(&"subtitle".to_string()), "{names:?}");
        // The sized overlays are as large as asked.
        let (small, large) = (
            build(TextStyle::Highlight, "", "", [255, 0, 0], 3_000_000, 0.2).unwrap(),
            build(TextStyle::Highlight, "", "", [255, 0, 0], 3_000_000, 0.6).unwrap(),
        );
        assert!(small.sized && (large.bounds.w / small.bounds.w - 3.0).abs() < 0.01);
        // The shortest and the longest items, empty words, and junk: all built, none a panic.
        let junk = "\u{0}\n\t ".repeat(50) + &"word ".repeat(400) + "+++";
        for style in ALL {
            for len in [0, 1, 300_000, 3_600_000_000, u64::MAX] {
                for said in ["", "A", junk.as_str()] {
                    let g = build(style, said, said, [0, 0, 0], len, f32::NAN).unwrap();
                    let _ = covered(&g, 3.0);
                }
            }
        }
    }

    #[test]
    fn words_are_on_top_of_their_plates() {
        // The headline is dark on a white plate: if the plate were over it, there'd be no ink.
        let g = build(TextStyle::NewsBreaking, "HEADLINE", "", [208, 2, 27], 5_000_000, 0.3).unwrap();
        let mut pix = Pixmap::new(1920, 1080).unwrap();
        g.animation.render_frame(&mut pix, 75.0, Transform::identity());
        let ink = pix.pixels().iter().filter(|p| p.alpha() > 250 && p.red() < 40 && p.green() < 40 && p.blue() < 50).count();
        assert!(ink > 3000, "its dark letters are seen on its white plate: {ink}");
        // And every card of the library is drawn.
        let back = screengrabcraft_model::Raster::filled(256, 144, screengrabcraft_model::Color::rgb(40, 60, 110)).unwrap();
        for style in ALL {
            let (title, sub) = sample(style);
            let c = card(style, title, sub, [11, 95, 255], 5_000_000, 0.3, &back).unwrap();
            assert_ne!(c.pixels(), back.pixels(), "{style:?}");
        }
    }
}

#[cfg(test)]
mod shots {
    use tiny_skia::{Pixmap, Transform};

    use super::*;

    /// Pictures of the graphics for looking at (only when `SGC_SHOTS` names a folder): sheets
    /// of them over a plain backdrop.
    #[test]
    fn sheets_of_the_graphics() {
        let Ok(dir) = std::env::var("SGC_SHOTS") else { return };
        let sheet = |name: &str, styles: &[(TextStyle, &str, &str, [u8; 3], f32)]| {
            let (w, h) = (960u32, 540u32);
            let mut all = Pixmap::new(w * 2, h * styles.len().div_ceil(2) as u32).unwrap();
            for (k, (style, title, sub, colour, at)) in styles.iter().enumerate() {
                let mut pix = Pixmap::new(w, h).unwrap();
                for (i, p) in pix.pixels_mut().iter_mut().enumerate() {
                    let (x, y) = ((i as u32 % w) as f32 / w as f32, (i as u32 / w) as f32 / h as f32);
                    *p = tiny_skia::PremultipliedColorU8::from_rgba(
                        (70.0 + 60.0 * x) as u8,
                        (95.0 + 50.0 * y) as u8,
                        (120.0 + 50.0 * (1.0 - x)) as u8,
                        255,
                    )
                    .unwrap();
                }
                let g = build(*style, title, sub, *colour, 6_000_000, 0.3).unwrap();
                g.animation.render(&mut pix, *at, Transform::from_scale(0.5, 0.5));
                all.draw_pixmap(
                    (k as u32 % 2 * w) as i32,
                    (k as u32 / 2 * h) as i32,
                    pix.as_ref(),
                    &tiny_skia::PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }
            let pic = screengrabcraft_render::canvas::from_pixmap(&all).unwrap();
            let png = screengrabcraft_render::codec::encode(&pic, screengrabcraft_model::ImageFormat::Png, 90).unwrap();
            std::fs::write(format!("{dir}/{name}.png"), png).unwrap();
        };
        let (red, blue, amber, teal) = ([0xd0, 0x02, 0x1b], [0x0b, 0x5f, 0xff], [0xf5, 0xa5, 0x24], [0x22, 0xc1, 0xa4]);
        use TextStyle::*;
        sheet(
            "stock-lower",
            &[
                (NewsBreaking, "Storm warning issued for the coast", "Winds to 70 mph expected by nightfall", red, 3.0),
                (NewsHeadline, "Council approves the new budget", "The vote passed 7 to 2 after a long night", blue, 3.0),
                (NewsName, "Alex Morgan", "Senior Correspondent", blue, 3.0),
                (LowerBar, "Alex Morgan", "Senior Correspondent", blue, 3.0),
                (LowerLine, "Alex Morgan", "Senior Correspondent", teal, 3.0),
                (LowerTag, "Alex Morgan", "Senior Correspondent", amber, 3.0),
            ],
        );
        sheet(
            "stock-news",
            &[
                (NewsLive, "City Hall", "9:41 AM", red, 3.0),
                (NewsTicker, "Latest", "Markets close higher for a third day     Roadworks begin on the ring road Monday", red, 3.0),
                (NewsLocation, "Downtown", "Tuesday, 9:41 AM", amber, 3.0),
                (NewsQuote, "We said we would finish the bridge by spring, and we have.", "Dana Reyes, City Engineer", teal, 3.0),
                (NewsStat, "72%", "of those asked agreed", blue, 3.0),
                (TitleBroadcast, "The Morning Report", "With Alex Morgan", blue, 3.0),
            ],
        );
        sheet(
            "stock-callouts",
            &[
                (Callout, "Click here to start a new recording", "", blue, 3.0),
                (Note, "Remember to save before you close", "", [0xff, 0xd8, 0x4d], 3.0),
                (Leader, "The record button", "", blue, 3.0),
                (Step, "2", "Choose where it goes", teal, 3.0),
                (Keys, "Ctrl + Shift + S", "", blue, 3.0),
                (Highlight, "", "", red, 3.0),
                (Circle, "", "", red, 3.0),
                (Spotlight, "", "", [0, 0, 0], 3.0),
                (TitleBox, "Chapter One", "", blue, 3.0),
                (Banner, "Getting started", "", teal, 3.0),
            ],
        );
        sheet(
            "stock-stingers",
            &[
                (StingerDiagonal, "", "", blue, 1.9),
                (StingerChevron, "", "", red, 1.9),
                (StingerClock, "", "", teal, 1.6),
                (StingerShutter, "", "", amber, 2.0),
                (StingerBlinds, "", "", blue, 1.9),
                (StingerIris, "", "", amber, 2.2),
                (StingerBars, "", "", blue, 1.9),
                (StingerPanel, "", "", red, 1.9),
            ],
        );
    }
}
