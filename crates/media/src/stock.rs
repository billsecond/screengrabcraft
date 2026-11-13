//! The library's broadcast graphics: news lower thirds, a live bug, a ticker, tags, cards and
//! full-screen stinger transitions. Each is built here as a Lottie animation (see
//! `screengrabcraft_render::lottie`, which plays them: the same player plays the Lottie files
//! brought in from elsewhere), sized to its words and as long as its item: it comes on over
//! its first moments, stays, and goes over its last.
//!
//! All of it is original: boxes, lines and words, drawn by this code. Nothing here is taken
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

const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
const DARK: [f32; 3] = [0.063, 0.071, 0.094];
const SOFT: [f32; 3] = [0.80, 0.82, 0.86];

/// How a value gets from one keyframe to the next.
#[derive(Clone, Copy)]
enum E {
    /// Quick, then settling.
    Out,
    /// Slow, then quick.
    In,
    Lin,
}

/// An animated property: each value with its frame and how it moves on to the next.
fn keys(k: &[(f32, Vec<f32>, E)]) -> Value {
    let mut out = Vec::with_capacity(k.len());
    let mut last = f32::MIN;
    for (t, v, e) in k {
        // Frames in order, whatever the lengths asked for.
        let t = t.max(last + 0.01);
        last = t;
        let (o, i) = match e {
            E::Out => ([0.2, 1.0], [0.3, 1.0]),
            E::In => ([0.7, 0.0], [0.8, 0.0]),
            E::Lin => ([0.0, 0.0], [1.0, 1.0]),
        };
        out.push(json!({"t": t, "s": v, "o": {"x": [o[0]], "y": [o[1]]}, "i": {"x": [i[0]], "y": [i[1]]}}));
    }
    json!({"a": 1, "k": out})
}

fn fixed(v: &[f32]) -> Value {
    json!({"a": 0, "k": v})
}

/// The colour as Lottie writes it.
fn rgb(c: [u8; 3]) -> [f32; 3] {
    [f32::from(c[0]) / 255.0, f32::from(c[1]) / 255.0, f32::from(c[2]) / 255.0]
}

/// A darker shade of a colour.
fn shade(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

/// How wide words are at a size.
fn width(words: &str, size: f32, bold: bool) -> f32 {
    let style = Style { font_size: 64.0, bold, ..Style::default() };
    text::layout(words, &style, None).width * size / 64.0
}

/// Words made safe for a graphic: one line, not too long.
fn line(words: &str, most: usize) -> String {
    let s: String = words.split_whitespace().collect::<Vec<_>>().join(" ");
    s.chars().take(most).collect()
}

/// Words broken into lines no wider than `max`, at most `lines` of them.
fn wrap(words: &str, size: f32, bold: bool, max: f32, lines: usize) -> String {
    let mut out: Vec<String> = Vec::new();
    for word in words.split_whitespace().take(200) {
        match out.last_mut() {
            Some(l) if width(&format!("{l} {word}"), size, bold) <= max => {
                l.push(' ');
                l.push_str(word);
            }
            _ => out.push(word.to_string()),
        }
    }
    out.truncate(lines.max(1));
    out.join("\n")
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
            o.insert("ip".into(), json!(0));
            o.insert("op".into(), json!(self.total));
        }
        self.layers.push(layer);
    }

    /// From 0 to 1 over `from..to` as it comes on, and back over the going (a little sooner for
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

    /// A box that wipes on from its left edge (from its top if `down`).
    fn bar(&mut self, r: Rect, radius: f32, colour: [f32; 3], alpha: f32, from: f32, to: f32, down: bool) {
        let (off, on) = if down { (vec![100.0, 0.0], vec![100.0, 100.0]) } else { (vec![0.0, 100.0], vec![100.0, 100.0]) };
        let anchor = if down { [r.x + r.w / 2.0, r.y] } else { [r.x, r.y + r.h / 2.0] };
        let ks = json!({"a": fixed(&anchor), "p": fixed(&anchor), "s": self.on_off(from, to, off, on)});
        self.push(json!({"ty": 4, "nm": "bar", "ks": ks, "shapes": [boxed(r, radius, colour, alpha)]}));
    }

    /// A box that's simply there (a matte's shape, or a backdrop that moves with its layer).
    fn still(&mut self, name: &str, r: Rect, colour: [f32; 3], alpha: f32, ks: Value, matte: bool) {
        let mut l = json!({"ty": 4, "nm": name, "ks": ks, "shapes": [boxed(r, 0.0, colour, alpha)]});
        if matte && let Some(o) = l.as_object_mut() {
            o.insert("td".into(), json!(1));
        }
        self.push(l);
    }

    /// Words that rise into a window (`clip`) as they come on and sink out of it as they go.
    /// `x`, `y`: where their first line's baseline starts (its middle if `centred`).
    #[allow(clippy::too_many_arguments)]
    fn words(&mut self, name: &str, words: &str, size: f32, bold: bool, colour: [f32; 3], x: f32, y: f32, centred: bool, clip: Rect, from: f32) {
        if words.trim().is_empty() {
            return;
        }
        self.still("window", clip, WHITE, 1.0, json!({}), true);
        let rise = clip.h.max(size) * 1.05;
        let ks = json!({"p": self.on_off(from, from + 12.0, vec![x, y + rise], vec![x, y])});
        let doc = json!({"s": size, "f": if bold { "Sans-Bold" } else { "Sans" }, "t": words, "j": if centred { 2 } else { 0 }, "fc": colour, "lh": size * 1.22});
        self.push(json!({"ty": 5, "nm": name, "tt": 1, "ks": ks, "t": {"d": {"k": [{"s": doc, "t": 0}]}}}));
    }

    fn done(self) -> Option<Animation> {
        Animation::from_value(
            json!({"v": "5.7.4", "nm": "ScreenGrabCraft", "fr": FPS, "ip": 0, "op": self.total, "w": W, "h": H, "layers": self.layers}),
        )
        .ok()
    }
}

/// A filled box as a shape group.
fn boxed(r: Rect, radius: f32, colour: [f32; 3], alpha: f32) -> Value {
    json!({"ty": "gr", "it": [
        {"ty": "rc", "p": fixed(&[r.x + r.w / 2.0, r.y + r.h / 2.0]), "s": fixed(&[r.w, r.h]), "r": fixed(&[radius])},
        {"ty": "fl", "c": fixed(&colour), "o": fixed(&[alpha * 100.0])},
        {"ty": "tr", "p": fixed(&[0.0, 0.0]), "a": fixed(&[0.0, 0.0]), "s": fixed(&[100.0, 100.0]), "r": fixed(&[0.0]), "o": fixed(&[100.0])}]})
}

/// A stock graphic: its animation, and the box it takes up at 1920 × 1080 (for choosing and
/// moving it).
pub struct Graphic {
    pub animation: Animation,
    pub bounds: Rect,
}

/// Whether a style is one of the graphics built here.
pub fn is_stock(style: TextStyle) -> bool {
    use TextStyle::*;
    matches!(
        style,
        NewsBreaking
            | NewsHeadline
            | NewsName
            | NewsLive
            | NewsTicker
            | NewsLocation
            | NewsQuote
            | NewsStat
            | TitleBroadcast
            | StingerBars
            | StingerIris
            | StingerSlices
            | StingerPanel
    )
}

/// Whether it's a stinger: a transition that covers the whole picture half-way through.
pub fn is_stinger(style: TextStyle) -> bool {
    use TextStyle::*;
    matches!(style, StingerBars | StingerIris | StingerSlices | StingerPanel)
}

/// Builds a stock graphic `len_us` long with these words and this colour; none for a style
/// that isn't one.
pub fn build(style: TextStyle, title: &str, subtitle: &str, accent: [u8; 3], len_us: u64) -> Option<Graphic> {
    let total = (len_us.min(3_600_000_000) as f32 / 1e6 * FPS).max(OUT * 2.0 + 2.0);
    let a = rgb(accent);
    let (title, sub) = (line(title, 120), line(subtitle, 400));
    let mut b = Build::new(total);
    let r = Rect::new;
    let bounds = match style {
        TextStyle::NewsBreaking => {
            let head = title.to_uppercase();
            let tab_w = width("BREAKING NEWS", 30.0, true) + 44.0;
            let hw = (width(&head, 56.0, true) + 64.0).clamp(640.0, 1728.0);
            let sw = if sub.is_empty() { 0.0 } else { (width(&sub, 32.0, false) + 64.0).clamp(hw * 0.6, 1728.0) };
            b.words("label", "BREAKING NEWS", 30.0, true, WHITE, 118.0, 825.0, false, r(96.0, 786.0, tab_w, 56.0), 4.0);
            b.words("title", &head, 56.0, true, DARK, 128.0, 908.0, false, r(96.0, 842.0, hw, 92.0), 8.0);
            b.words("subtitle", &sub, 32.0, false, WHITE, 128.0, 974.0, false, r(96.0, 934.0, sw, 58.0), 12.0);
            b.bar(r(84.0, 786.0, 12.0, if sub.is_empty() { 148.0 } else { 206.0 }), 0.0, a, 1.0, 0.0, 10.0, true);
            b.bar(r(96.0, 786.0, tab_w, 56.0), 0.0, a, 1.0, 0.0, 10.0, false);
            b.bar(r(96.0, 842.0, hw, 92.0), 0.0, WHITE, 1.0, 4.0, 16.0, false);
            if !sub.is_empty() {
                b.bar(r(96.0, 934.0, sw, 58.0), 0.0, DARK, 0.94, 8.0, 20.0, false);
            }
            r(84.0, 786.0, hw.max(sw) + 12.0, if sub.is_empty() { 148.0 } else { 206.0 })
        }
        TextStyle::NewsHeadline => {
            b.words("title", &title, 60.0, true, WHITE, 72.0, 944.0, false, r(48.0, 884.0, 1824.0, 76.0), 8.0);
            b.words("subtitle", &sub, 34.0, false, SOFT, 72.0, 996.0, false, r(48.0, 960.0, 1824.0, 52.0), 12.0);
            b.bar(r(0.0, 876.0, W, 6.0), 0.0, a, 1.0, 4.0, 20.0, false);
            b.bar(r(0.0, 882.0, 24.0, 142.0), 0.0, a, 1.0, 0.0, 10.0, true);
            b.bar(r(0.0, 882.0, W, 142.0), 0.0, DARK, 0.93, 0.0, 12.0, true);
            r(0.0, 876.0, W, 148.0)
        }
        TextStyle::NewsName => {
            let nw = (width(&title, 46.0, true) + 56.0).clamp(280.0, 1500.0);
            let rw = if sub.is_empty() { 0.0 } else { (width(&sub, 30.0, false) + 56.0).clamp(220.0, 1500.0) };
            b.words("title", &title, 46.0, true, WHITE, 132.0, 889.0, false, r(104.0, 838.0, nw, 70.0), 6.0);
            b.words("subtitle", &sub, 30.0, false, WHITE, 132.0, 943.0, false, r(104.0, 908.0, rw, 50.0), 10.0);
            b.bar(r(92.0, 838.0, 8.0, if sub.is_empty() { 70.0 } else { 120.0 }), 0.0, WHITE, 1.0, 0.0, 8.0, true);
            b.bar(r(104.0, 838.0, nw, 70.0), 0.0, a, 1.0, 2.0, 14.0, false);
            if !sub.is_empty() {
                b.bar(r(104.0, 908.0, rw, 50.0), 0.0, DARK, 0.94, 6.0, 18.0, false);
            }
            r(92.0, 838.0, nw.max(rw) + 12.0, if sub.is_empty() { 70.0 } else { 120.0 })
        }
        TextStyle::NewsLive => {
            let place = if sub.is_empty() { title.clone() } else { format!("{title}   {sub}") };
            let lw = if place.is_empty() { 0.0 } else { width(&place, 30.0, false) + 48.0 };
            b.words("label", "LIVE", 32.0, true, WHITE, 150.0, 111.0, false, r(96.0, 72.0, 150.0, 56.0), 2.0);
            b.words("title", &place, 30.0, false, WHITE, 270.0, 110.0, false, r(246.0, 72.0, lw, 56.0), 8.0);
            // The dot, beating once a second.
            let mut beat = vec![(0.0, vec![100.0], E::Lin)];
            let mut t = 15.0;
            while t < total && beat.len() < 400 {
                beat.push((t, vec![if beat.len() % 2 == 1 { 25.0 } else { 100.0 }], E::Lin));
                t += 15.0;
            }
            let dot = json!({"ty": "gr", "it": [
                {"ty": "el", "p": fixed(&[124.0, 100.0]), "s": fixed(&[18.0, 18.0])},
                {"ty": "fl", "c": fixed(&WHITE), "o": keys(&beat)},
                {"ty": "tr", "p": fixed(&[0.0, 0.0]), "a": fixed(&[0.0, 0.0]), "s": fixed(&[100.0, 100.0]), "r": fixed(&[0.0]), "o": b.on_off(4.0, 10.0, vec![0.0], vec![100.0])}]});
            b.push(json!({"ty": 4, "nm": "dot", "ks": {}, "shapes": [dot]}));
            b.bar(r(96.0, 72.0, 150.0, 56.0), 6.0, a, 1.0, 0.0, 9.0, false);
            if lw > 0.0 {
                b.bar(r(240.0, 72.0, lw + 6.0, 56.0), 0.0, DARK, 0.92, 4.0, 15.0, false);
            }
            r(96.0, 72.0, 150.0 + lw, 56.0)
        }
        TextStyle::NewsTicker => {
            let label = title.to_uppercase();
            let lbw = (width(&label, 30.0, true) + 56.0).clamp(120.0, 600.0);
            // The words pass at a steady pace, again and again.
            let crawl = if sub.is_empty() {
                String::new()
            } else {
                std::iter::repeat_n(sub.as_str(), 6).collect::<Vec<_>>().join("     •     ").chars().take(1500).collect()
            };
            b.words("title", &label, 30.0, true, WHITE, 28.0, 1043.0, false, r(0.0, 1000.0, lbw, 64.0), 4.0);
            if !crawl.is_empty() {
                b.still("window", r(lbw, 1000.0, W - lbw, 64.0), WHITE, 1.0, json!({}), true);
                let ks = json!({"p": keys(&[(0.0, vec![lbw + 48.0, 1043.0], E::Lin), (total, vec![lbw + 48.0 - 160.0 * total / FPS, 1043.0], E::Lin)]), "o": b.on_off(0.0, 8.0, vec![0.0], vec![100.0])});
                let doc = json!({"s": 30.0, "f": "Sans", "t": crawl, "j": 0, "fc": WHITE});
                b.push(json!({"ty": 5, "nm": "subtitle", "tt": 1, "ks": ks, "t": {"d": {"k": [{"s": doc, "t": 0}]}}}));
            }
            b.bar(r(0.0, 1000.0, lbw, 64.0), 0.0, a, 1.0, 2.0, 12.0, false);
            b.bar(r(0.0, 996.0, W, 4.0), 0.0, a, 1.0, 0.0, 14.0, false);
            b.bar(r(0.0, 1000.0, W, 64.0), 0.0, DARK, 0.95, 0.0, 10.0, true);
            r(0.0, 996.0, W, 68.0)
        }
        TextStyle::NewsLocation => {
            let bw = (width(&title, 40.0, true).max(width(&sub, 28.0, false)) + 56.0).clamp(200.0, 1400.0);
            let bh = if sub.is_empty() { 60.0 } else { 96.0 };
            b.words("title", &title, 40.0, true, WHITE, 134.0, 116.0, false, r(106.0, 72.0, bw, 54.0), 4.0);
            b.words("subtitle", &sub, 28.0, false, SOFT, 134.0, 154.0, false, r(106.0, 126.0, bw, 42.0), 8.0);
            b.bar(r(96.0, 72.0, 10.0, bh), 0.0, a, 1.0, 0.0, 8.0, true);
            b.bar(r(106.0, 72.0, bw, bh), 0.0, DARK, 0.86, 2.0, 14.0, false);
            r(96.0, 72.0, bw + 10.0, bh)
        }
        TextStyle::NewsQuote => {
            let said = wrap(&title, 50.0, false, 960.0, 4);
            let n = said.lines().count().max(1) as f32;
            let ch = 190.0 + n * 61.0 + if sub.is_empty() { 0.0 } else { 70.0 };
            let top = (H - ch) / 2.0;
            let card = r(360.0, top, 1200.0, ch);
            b.words("title", &said, 50.0, false, WHITE, 520.0, top + 150.0, false, r(360.0, top + 96.0, 1200.0, n * 61.0 + 20.0), 8.0);
            if !sub.is_empty() {
                b.words("subtitle", &format!("— {sub}"), 32.0, true, a, 520.0, top + ch - 46.0, false, r(360.0, top + ch - 90.0, 1200.0, 60.0), 14.0);
            }
            b.words("mark", "“", 180.0, true, a, 400.0, top + 196.0, false, r(360.0, top, 150.0, 150.0), 4.0);
            b.bar(r(360.0, top, 10.0, ch), 0.0, a, 1.0, 0.0, 10.0, true);
            b.bar(card, 0.0, DARK, 0.93, 0.0, 14.0, false);
            card
        }
        TextStyle::NewsStat => {
            let bw = (width(&title, 150.0, true).max(width(&sub, 36.0, false)) + 96.0).clamp(360.0, 900.0);
            let x = W - 96.0 - bw;
            let card = r(x, 300.0, bw, 300.0);
            b.words("title", &title, 150.0, true, WHITE, x + bw / 2.0, 484.0, true, r(x, 316.0, bw, 190.0), 6.0);
            b.words("subtitle", &sub, 36.0, false, WHITE, x + bw / 2.0, 556.0, true, r(x, 512.0, bw, 62.0), 10.0);
            b.bar(r(x, 588.0, bw, 12.0), 0.0, shade(a, 0.62), 1.0, 4.0, 16.0, false);
            b.bar(card, 0.0, a, 1.0, 0.0, 12.0, true);
            card
        }
        TextStyle::TitleBroadcast => {
            let tw = width(&title, 96.0, true).max(width(&sub, 44.0, false)).clamp(300.0, 1700.0);
            let x = W / 2.0;
            b.words("title", &title, 96.0, true, WHITE, x, 520.0, true, r(x - tw / 2.0 - 30.0, 420.0, tw + 60.0, 128.0), 4.0);
            b.words("subtitle", &sub, 44.0, false, SOFT, x, 622.0, true, r(x - tw / 2.0 - 30.0, 572.0, tw + 60.0, 70.0), 10.0);
            // A line under the title, drawn on from its middle... its left, as a bar.
            b.bar(r(x - tw / 2.0, 552.0, tw, 8.0), 4.0, a, 1.0, 6.0, 22.0, false);
            b.bar(r(x - tw / 2.0 - 60.0, 396.0, tw + 120.0, if sub.is_empty() { 196.0 } else { 270.0 }), 0.0, DARK, 0.55, 0.0, 12.0, true);
            r(x - tw / 2.0 - 60.0, 396.0, tw + 120.0, if sub.is_empty() { 196.0 } else { 270.0 })
        }
        TextStyle::StingerBars | TextStyle::StingerSlices | TextStyle::StingerPanel => {
            // Pieces that cross the picture one after another: all on it half-way through.
            let (n, across) = match style {
                TextStyle::StingerBars => (5usize, true),
                TextStyle::StingerSlices => (8, false),
                _ => (2, true),
            };
            let (mid, lag) = (b.total / 2.0, b.total * 0.035);
            for k in 0..n {
                let (start, end) = (k as f32 * lag, b.total - (n - 1 - k) as f32 * lag);
                let rect = match style {
                    TextStyle::StingerBars => r(0.0, k as f32 * H / n as f32, W, H / n as f32 + 1.0),
                    TextStyle::StingerSlices => r(k as f32 * W / n as f32, 0.0, W / n as f32 + 1.0, H),
                    _ => r(0.0, 0.0, W, H),
                };
                let colour = match (style, k % 2) {
                    (TextStyle::StingerPanel, 0) => a,
                    (TextStyle::StingerPanel, _) => DARK,
                    (_, 0) => a,
                    _ => shade(a, 0.72),
                };
                let (from, to) = if across { (vec![-W, 0.0], vec![W, 0.0]) } else { (vec![0.0, -H], vec![0.0, H]) };
                let hold = b.total * 0.06;
                let p = keys(&[
                    (start, from, E::Out),
                    (mid - hold + k as f32 * 0.01, vec![0.0, 0.0], E::Lin),
                    (mid + hold, vec![0.0, 0.0], E::In),
                    (end, to, E::Lin),
                ]);
                b.still("piece", rect, colour, 1.0, json!({"p": p}), false);
            }
            r(0.0, 0.0, W, H)
        }
        TextStyle::StingerIris => {
            // A disc grows to cover the picture; then a hole grows in the colour, uncovering it.
            let mid = b.total / 2.0;
            let disc = |from: f32, to: f32| {
                json!({"ty": "gr", "it": [
                    {"ty": "el", "p": fixed(&[W / 2.0, H / 2.0]), "s": keys(&[(from, vec![0.0, 0.0], if from == 0.0 { E::In } else { E::Out }), (to, vec![2300.0, 2300.0], E::Lin)])},
                    {"ty": "fl", "c": fixed(&a), "o": fixed(&[100.0])},
                    {"ty": "tr", "p": fixed(&[0.0, 0.0]), "a": fixed(&[0.0, 0.0]), "s": fixed(&[100.0, 100.0]), "r": fixed(&[0.0]), "o": fixed(&[100.0])}]})
            };
            b.layers.push(json!({"ty": 4, "nm": "cover", "ind": 1, "ip": 0, "op": mid, "ks": {}, "shapes": [disc(0.0, mid)]}));
            b.layers.push(json!({"ty": 4, "nm": "hole", "ind": 2, "td": 1, "ip": mid, "op": b.total, "ks": {}, "shapes": [disc(mid, b.total)]}));
            b.layers.push(json!({"ty": 1, "nm": "colour", "ind": 3, "tt": 2, "ip": mid, "op": b.total, "ks": {}, "sw": W, "sh": H,
                "sc": format!("#{:02x}{:02x}{:02x}", accent[0], accent[1], accent[2])}));
            r(0.0, 0.0, W, H)
        }
        _ => return None,
    };
    Some(Graphic { animation: b.done()?, bounds })
}

/// A graphic as the library's card shows it, over `backdrop`: close up, as it is once it has come
/// on (a stinger part-way across the whole of it).
pub fn card(
    style: TextStyle,
    title: &str,
    subtitle: &str,
    accent: [u8; 3],
    len_us: u64,
    backdrop: &screengrabcraft_model::Raster,
) -> Option<screengrabcraft_model::Raster> {
    use tiny_skia::Transform;
    let g = build(style, title, subtitle, accent, len_us)?;
    let mut pix = screengrabcraft_render::canvas::to_pixmap(backdrop)?;
    let (w, h) = (pix.width() as f32, pix.height() as f32);
    let secs = len_us.min(3_600_000_000) as f32 / 1e6;
    if is_stinger(style) {
        g.animation.render(&mut pix, secs * 0.3, Transform::from_scale(w / W, h / H));
    } else {
        let b = g.bounds;
        let k = ((w - 16.0) / b.w.max(1.0)).min((h - 16.0) / b.h.max(1.0)).min(0.6);
        let at = Transform::from_translate(-(b.x + b.w / 2.0), -(b.y + b.h / 2.0)).post_scale(k, k).post_translate(w / 2.0, h / 2.0);
        g.animation.render(&mut pix, secs / 2.0, at);
    }
    screengrabcraft_render::canvas::from_pixmap(&pix)
}

#[cfg(test)]
mod tests {
    use tiny_skia::{Pixmap, Transform};

    use super::*;

    const ALL: [TextStyle; 13] = [
        TextStyle::NewsBreaking,
        TextStyle::NewsHeadline,
        TextStyle::NewsName,
        TextStyle::NewsLive,
        TextStyle::NewsTicker,
        TextStyle::NewsLocation,
        TextStyle::NewsQuote,
        TextStyle::NewsStat,
        TextStyle::TitleBroadcast,
        TextStyle::StingerBars,
        TextStyle::StingerIris,
        TextStyle::StingerSlices,
        TextStyle::StingerPanel,
    ];

    /// How much of the picture is drawn on at a frame.
    fn covered(g: &Graphic, frame: f32) -> f32 {
        let mut pix = Pixmap::new(480, 270).unwrap();
        g.animation.render_frame(&mut pix, frame, Transform::from_scale(0.25, 0.25));
        pix.pixels().iter().filter(|p| p.alpha() > 128).count() as f32 / (480.0 * 270.0)
    }

    #[test]
    fn every_graphic_comes_on_stays_and_goes() {
        for style in ALL {
            assert!(is_stock(style));
            let g = build(style, "City Council Approves Budget", "Vote passes 7 to 2 after a long night", [208, 2, 27], 5_000_000);
            assert!(g.is_some(), "{style:?}");
            let g = g.unwrap();
            assert_eq!(g.animation.duration_us(), 5_000_000, "{style:?}");
            let (start, held, end) = (covered(&g, 0.0), covered(&g, 75.0), covered(&g, 149.9));
            assert!(start < 0.01 && end < 0.02, "{style:?}: nothing at its start or its end ({start}, {end})");
            assert!(g.bounds.w > 10.0 && g.bounds.h > 10.0 && g.bounds.x >= 0.0 && g.bounds.x + g.bounds.w <= W + 0.5, "{style:?} {:?}", g.bounds);
            if is_stinger(style) {
                assert!(held > 0.99, "{style:?} covers the picture half-way: {held}");
            } else {
                assert!(held > 0.005 && held < 0.6, "{style:?} is on the picture, not over all of it: {held}");
            }
        }
        assert!(build(TextStyle::Title, "x", "", [0, 0, 0], 1_000_000).is_none());
    }

    #[test]
    fn a_graphic_fits_its_words_and_takes_any_length_and_any_words() {
        let short = build(TextStyle::NewsBreaking, "Fire", "", [208, 2, 27], 4_000_000).unwrap();
        let long =
            build(TextStyle::NewsBreaking, "Wildfire forces thousands to leave their homes", "Crews work through the night", [208, 2, 27], 4_000_000)
                .unwrap();
        assert!(long.bounds.w > short.bounds.w + 300.0 && long.bounds.h > short.bounds.h, "{:?} {:?}", short.bounds, long.bounds);
        // Its words are there to change, by name, as in any Lottie file.
        let names: Vec<String> = long.animation.texts().into_iter().map(|(n, _)| n).collect();
        assert!(names.contains(&"title".to_string()) && names.contains(&"subtitle".to_string()), "{names:?}");
        // The shortest and the longest items, empty words, and junk: all built, none a panic.
        let junk = "\u{0}\n\t ".repeat(50) + &"word ".repeat(400);
        for style in ALL {
            for len in [0, 1, 300_000, 3_600_000_000, u64::MAX] {
                for words in ["", "A", junk.as_str()] {
                    let g = build(style, words, words, [0, 0, 0], len).unwrap();
                    let _ = covered(&g, 3.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod shots {
    use tiny_skia::{Pixmap, Transform};

    use super::*;

    /// Pictures of the graphics for looking at (only when `SGC_SHOTS` names a folder): a sheet
    /// of them once they've come on, over a plain backdrop.
    #[test]
    fn a_sheet_of_the_graphics() {
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
                let g = build(*style, title, sub, *colour, 6_000_000).unwrap();
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
        sheet(
            "stock-news",
            &[
                (TextStyle::NewsBreaking, "Storm warning issued for the coast", "Winds to 70 mph expected by nightfall", red, 3.0),
                (TextStyle::NewsHeadline, "Council approves the new budget", "The vote passed 7 to 2 after a long night", blue, 3.0),
                (TextStyle::NewsName, "Alex Morgan", "Senior Correspondent", blue, 3.0),
                (TextStyle::NewsLive, "City Hall", "9:41 AM", red, 3.0),
                (TextStyle::NewsTicker, "Latest", "Markets close higher for a third day     Roadworks begin on the ring road Monday", red, 3.0),
                (TextStyle::NewsLocation, "Downtown", "Tuesday, 9:41 AM", amber, 3.0),
            ],
        );
        sheet(
            "stock-more",
            &[
                (TextStyle::NewsQuote, "We said we would finish the bridge by spring, and we have.", "Dana Reyes, City Engineer", teal, 3.0),
                (TextStyle::NewsStat, "72%", "of those asked agreed", blue, 3.0),
                (TextStyle::TitleBroadcast, "The Morning Report", "With Alex Morgan", blue, 3.0),
                (TextStyle::NewsBreaking, "Storm warning issued for the coast", "Winds to 70 mph expected by nightfall", red, 0.25),
                (TextStyle::StingerBars, "", "", blue, 2.2),
                (TextStyle::StingerIris, "", "", amber, 3.9),
            ],
        );
    }
}
