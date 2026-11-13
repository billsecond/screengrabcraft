//! A player for Lottie animations: the Bodymovin JSON that After Effects, Glaxnimate, LottieFiles
//! and many other tools write, and that open-source lower thirds, titles and transitions come
//! in. Drawn with tiny-skia, a frame at a time, onto a see-through picture.
//!
//! What's played: shape layers (rectangles, ellipses, paths; fills, strokes, gradients; groups
//! with their transforms; trimmed paths), solid layers, text layers (in this program's own
//! fonts), precompositions, null layers and parenting, masks, alpha track mattes, and animated
//! properties with their easing and holds. What isn't: expressions, effects, images, text
//! animators, repeaters and merged paths (such things are left out, the rest is drawn).
//!
//! The file is someone else's: its size, its layers, their nesting and every path are capped,
//! numbers that aren't numbers are left out, and nothing in it can make this panic.

use serde_json::Value;
use tiny_skia::{
    FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Mask, MaskType, Paint, Path, PathBuilder, PathSegment, Pixmap, PixmapPaint, Point,
    RadialGradient, Shader, SpreadMode, Stroke, Transform,
};

use crate::text;

/// The largest file read.
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_LAYERS: usize = 300;
const MAX_ITEMS: usize = 300;
const MAX_POINTS: usize = 4000;
const MAX_DEPTH: usize = 6;
const MAX_SIDE: f32 = 8192.0;

/// A Lottie animation, ready to be drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    root: Value,
    pub width: f32,
    pub height: f32,
    /// Frames a second.
    pub fps: f32,
    /// Its first frame, and the one after its last.
    pub first: f32,
    pub last: f32,
}

fn f(v: &Value) -> Option<f32> {
    let n = match v {
        Value::Array(a) => a.first()?.as_f64()?,
        other => other.as_f64()?,
    } as f32;
    n.is_finite().then_some(n.clamp(-1.0e7, 1.0e7))
}

fn list(v: &Value) -> Vec<f32> {
    match v {
        Value::Array(a) => a.iter().take(64).filter_map(f).collect(),
        other => f(other).into_iter().collect(),
    }
}

fn items(v: &Value, most: usize) -> &[Value] {
    let a = v.as_array().map(Vec::as_slice).unwrap_or_default();
    a.get(..a.len().min(most)).unwrap_or_default()
}

/// The `y` of the easing curve from (0, 0) to (1, 1) with these handles, at `x`.
fn ease(x: f32, out: (f32, f32), into: (f32, f32)) -> f32 {
    let bez = |t: f32, a: f32, b: f32| 3.0 * (1.0 - t) * (1.0 - t) * t * a + 3.0 * (1.0 - t) * t * t * b + t * t * t;
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if bez(mid, out.0, into.0) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez((lo + hi) / 2.0, out.1, into.1)
}

/// Between which keyframes `frame` falls: the one before it, the one after, and how far between
/// them it is (eased); none before the first (the first's value) or for a held one.
fn between(keys: &[Value], frame: f32) -> (usize, Option<(usize, f32)>) {
    let t = |k: &Value| k.get("t").and_then(f).unwrap_or(0.0);
    let after = keys.iter().position(|k| t(k) > frame).unwrap_or(keys.len());
    let Some(i) = after.checked_sub(1) else { return (0, None) };
    let (Some(a), Some(b)) = (keys.get(i), keys.get(i + 1)) else { return (i.min(keys.len().saturating_sub(1)), None) };
    if a.get("h").and_then(f).is_some_and(|h| h >= 1.0) {
        return (i, None);
    }
    let span = (t(b) - t(a)).max(1.0e-4);
    let handle = |key: &str, or: (f32, f32)| {
        let h = a.get(key);
        let x = h.and_then(|h| h.get("x")).and_then(f);
        let y = h.and_then(|h| h.get("y")).and_then(f);
        x.zip(y).map_or(or, |(x, y)| (x.clamp(0.0, 1.0), y.clamp(-4.0, 4.0)))
    };
    let p = ((frame - t(a)) / span).clamp(0.0, 1.0);
    (i, Some((i + 1, ease(p, handle("o", (0.0, 0.0)), handle("i", (1.0, 1.0))))))
}

/// Whether a property's `k` is keyframes (not a plain value).
fn keyed(k: &Value) -> Option<&[Value]> {
    let a = k.as_array()?;
    a.first()?.as_object()?.contains_key("t").then_some(items(k, 2000))
}

/// A property's numbers at a frame (`or` when it isn't there).
fn prop(p: Option<&Value>, frame: f32, or: &[f32]) -> Vec<f32> {
    let Some(k) = p.and_then(|p| p.get("k")) else { return or.to_vec() };
    let Some(keys) = keyed(k) else {
        let v = list(k);
        return if v.is_empty() { or.to_vec() } else { v };
    };
    let start = |i: usize| keys.get(i).and_then(|k| k.get("s")).map(list).filter(|v| !v.is_empty());
    let (i, next) = between(keys, frame);
    // A last keyframe may have no value of its own: it ends where the one before it did.
    let from = start(i).or_else(|| i.checked_sub(1).and_then(|j| keys.get(j)).and_then(|k| k.get("e")).map(list)).unwrap_or_else(|| or.to_vec());
    let Some((j, p)) = next else { return from };
    let to = keys.get(i).and_then(|k| k.get("e")).map(list).filter(|v| !v.is_empty()).or_else(|| start(j)).unwrap_or_else(|| from.clone());
    from.iter().zip(to.iter().chain(std::iter::repeat(&0.0))).map(|(a, b)| a + (b - a) * p).collect()
}

fn one(p: Option<&Value>, frame: f32, or: f32) -> f32 {
    prop(p, frame, &[or]).first().copied().unwrap_or(or)
}

fn two(p: Option<&Value>, frame: f32, or: (f32, f32)) -> (f32, f32) {
    let v = prop(p, frame, &[or.0, or.1]);
    (v.first().copied().unwrap_or(or.0), v.get(1).copied().unwrap_or(or.1))
}

/// A transform (a layer's `ks`, a group's `tr`) and its opacity at a frame.
fn xform(ks: Option<&Value>, frame: f32) -> (Transform, f32) {
    let Some(ks) = ks else { return (Transform::identity(), 1.0) };
    let a = two(ks.get("a"), frame, (0.0, 0.0));
    let p = match ks.get("p") {
        Some(p) if p.get("s").and_then(Value::as_bool).unwrap_or(false) => (one(p.get("x"), frame, 0.0), one(p.get("y"), frame, 0.0)),
        p => two(p, frame, (0.0, 0.0)),
    };
    let s = two(ks.get("s"), frame, (100.0, 100.0));
    let r = one(ks.get("r").or_else(|| ks.get("rz")), frame, 0.0);
    let o = one(ks.get("o"), frame, 100.0);
    let t =
        Transform::from_translate(-a.0, -a.1).post_scale(s.0 / 100.0, s.1 / 100.0).post_concat(Transform::from_rotate(r)).post_translate(p.0, p.1);
    (t, (o / 100.0).clamp(0.0, 1.0))
}

/// A path as Lottie holds it: its points, each with the handle into it and out of it.
#[derive(Clone, Debug, Default)]
struct Bezier {
    v: Vec<(f32, f32)>,
    i: Vec<(f32, f32)>,
    o: Vec<(f32, f32)>,
    closed: bool,
}

fn bezier_of(s: &Value) -> Option<Bezier> {
    // A keyframe holds its shape in a list of one.
    let s = if s.is_array() { s.get(0)? } else { s };
    let pts = |key: &str| -> Vec<(f32, f32)> {
        items(s.get(key).unwrap_or(&Value::Null), MAX_POINTS)
            .iter()
            .map(|p| (p.get(0).and_then(f).unwrap_or(0.0), p.get(1).and_then(f).unwrap_or(0.0)))
            .collect()
    };
    let v = pts("v");
    (!v.is_empty()).then(|| Bezier { v, i: pts("i"), o: pts("o"), closed: s.get("c").and_then(Value::as_bool).unwrap_or(false) })
}

/// A path property (`sh`'s `ks`, a mask's `pt`) at a frame.
fn bezier(p: Option<&Value>, frame: f32) -> Option<Bezier> {
    let k = p?.get("k")?;
    let Some(keys) = keyed(k) else { return bezier_of(k) };
    let start = |i: usize| keys.get(i).and_then(|k| k.get("s")).and_then(bezier_of);
    let (i, next) = between(keys, frame);
    let from = start(i).or_else(|| i.checked_sub(1).and_then(start))?;
    let Some((j, p)) = next else { return Some(from) };
    let Some(to) = keys.get(i).and_then(|k| k.get("e")).and_then(bezier_of).or_else(|| start(j)) else { return Some(from) };
    if to.v.len() != from.v.len() {
        return Some(from);
    }
    let mix = |a: &[(f32, f32)], b: &[(f32, f32)]| {
        a.iter().zip(b.iter().chain(std::iter::repeat(&(0.0, 0.0)))).map(|(a, b)| (a.0 + (b.0 - a.0) * p, a.1 + (b.1 - a.1) * p)).collect()
    };
    Some(Bezier { v: mix(&from.v, &to.v), i: mix(&from.i, &to.i), o: mix(&from.o, &to.o), closed: from.closed })
}

fn path_of(b: &Bezier) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let first = *b.v.first()?;
    pb.move_to(first.0, first.1);
    let at = |list: &[(f32, f32)], k: usize| list.get(k).copied().unwrap_or((0.0, 0.0));
    let n = b.v.len();
    for k in 1..=n {
        if k == n && !b.closed {
            break;
        }
        let (a, z) = (at(&b.v, k - 1), at(&b.v, k % n));
        let (c1, c2) = (at(&b.o, k - 1), at(&b.i, k % n));
        pb.cubic_to(a.0 + c1.0, a.1 + c1.1, z.0 + c2.0, z.1 + c2.1, z.0, z.1);
    }
    if b.closed {
        pb.close();
    }
    pb.finish()
}

/// A shape item that is a path (`rc`, `el`, `sh`), at a frame.
fn shape_path(item: &Value, frame: f32) -> Option<Path> {
    match item.get("ty")?.as_str()? {
        "rc" => {
            let (p, s) = (two(item.get("p"), frame, (0.0, 0.0)), two(item.get("s"), frame, (0.0, 0.0)));
            let r = one(item.get("r"), frame, 0.0).max(0.0);
            let rect = screengrabcraft_model::Rect::new(p.0 - s.0 / 2.0, p.1 - s.1 / 2.0, s.0.abs(), s.1.abs());
            crate::draw::rounded_rect(rect, r.min(s.0.abs() / 2.0).min(s.1.abs() / 2.0))
        }
        "el" => {
            let (p, s) = (two(item.get("p"), frame, (0.0, 0.0)), two(item.get("s"), frame, (0.0, 0.0)));
            PathBuilder::from_oval(tiny_skia::Rect::from_xywh(p.0 - s.0 / 2.0, p.1 - s.1 / 2.0, s.0.abs().max(0.01), s.1.abs().max(0.01))?)
        }
        "sh" => path_of(&bezier(item.get("ks"), frame)?),
        _ => None,
    }
}

/// A path as runs of straight lines (each with whether it closes).
fn flatten(path: &Path) -> Vec<(Vec<Point>, bool)> {
    let mut runs: Vec<(Vec<Point>, bool)> = Vec::new();
    let mut total = 0usize;
    for seg in path.segments() {
        if total > MAX_POINTS * 8 {
            break;
        }
        let last = runs.last().and_then(|r| r.0.last()).copied();
        match seg {
            PathSegment::MoveTo(p) => runs.push((vec![p], false)),
            PathSegment::LineTo(p) => {
                if let Some(r) = runs.last_mut() {
                    r.0.push(p);
                }
            }
            PathSegment::QuadTo(c, p) => {
                if let (Some(a), Some(r)) = (last, runs.last_mut()) {
                    for k in 1..=10 {
                        let t = k as f32 / 10.0;
                        let u = 1.0 - t;
                        r.0.push(Point::from_xy(u * u * a.x + 2.0 * u * t * c.x + t * t * p.x, u * u * a.y + 2.0 * u * t * c.y + t * t * p.y));
                    }
                }
            }
            PathSegment::CubicTo(c1, c2, p) => {
                if let (Some(a), Some(r)) = (last, runs.last_mut()) {
                    for k in 1..=14 {
                        let t = k as f32 / 14.0;
                        let u = 1.0 - t;
                        let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                        r.0.push(Point::from_xy(b0 * a.x + b1 * c1.x + b2 * c2.x + b3 * p.x, b0 * a.y + b1 * c1.y + b2 * c2.y + b3 * p.y));
                    }
                }
            }
            PathSegment::Close => {
                if let Some(r) = runs.last_mut() {
                    r.1 = true;
                    if let Some(first) = r.0.first().copied() {
                        r.0.push(first);
                    }
                }
            }
        }
        total += 14;
    }
    runs
}

/// The part of a path from `start` to `end` of its length (each 0 to 1, `end` may pass 1 to
/// run on round a closed path).
fn trimmed(path: &Path, start: f32, end: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    for (pts, _) in flatten(path) {
        let lens: Vec<f32> = pts.windows(2).map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt()).collect();
        let total: f32 = lens.iter().sum();
        if total <= 0.0 {
            continue;
        }
        let mut stretch = |from: f32, to: f32| {
            let (from, to) = (from * total, to * total);
            let (mut at, mut pen) = (0.0f32, false);
            for (k, len) in lens.iter().enumerate() {
                let (a, b) = (at, at + len);
                at = b;
                if b <= from || a >= to || *len <= 0.0 {
                    continue;
                }
                let (Some(p), Some(q)) = (pts.get(k), pts.get(k + 1)) else { continue };
                let along = |d: f32| Point::from_xy(p.x + (q.x - p.x) * ((d - a) / len), p.y + (q.y - p.y) * ((d - a) / len));
                let (s, e) = (along(from.max(a)), along(to.min(b)));
                if !pen {
                    pb.move_to(s.x, s.y);
                    pen = true;
                }
                pb.line_to(e.x, e.y);
            }
        };
        if end <= 1.0 {
            stretch(start, end);
        } else {
            stretch(start, 1.0);
            stretch(0.0, end - 1.0);
        }
    }
    pb.finish()
}

fn colour(c: &[f32], alpha: f32) -> tiny_skia::Color {
    let at = |k: usize| c.get(k).copied().unwrap_or(0.0).clamp(0.0, 1.0);
    tiny_skia::Color::from_rgba(at(0), at(1), at(2), alpha.clamp(0.0, 1.0)).unwrap_or(tiny_skia::Color::BLACK)
}

/// A fill's or a stroke's paint: a colour, or a gradient (`gf`, `gs`).
fn paint_of(item: &Value, frame: f32, alpha: f32) -> Paint<'static> {
    let mut paint = Paint { anti_alias: true, ..Paint::default() };
    let alpha = alpha * (one(item.get("o"), frame, 100.0) / 100.0).clamp(0.0, 1.0);
    let Some(g) = item.get("g") else {
        paint.set_color(colour(&prop(item.get("c"), frame, &[0.0, 0.0, 0.0]), alpha));
        return paint;
    };
    let n = g.get("p").and_then(f).unwrap_or(0.0).clamp(0.0, 32.0) as usize;
    let flat = prop(g.get("k"), frame, &[]);
    let colours: Vec<(f32, tiny_skia::Color)> =
        (0..n).filter_map(|k| flat.get(k * 4..k * 4 + 4)).map(|s| (s[0].clamp(0.0, 1.0), colour(&s[1..], alpha))).collect();
    let stops: Vec<GradientStop> = colours.iter().map(|(at, c)| GradientStop::new(*at, *c)).collect();
    let (s, e) = (two(item.get("s"), frame, (0.0, 0.0)), two(item.get("e"), frame, (100.0, 0.0)));
    let (a, b) = (Point::from_xy(s.0, s.1), Point::from_xy(e.0, e.1));
    let radial = item.get("t").and_then(f).is_some_and(|t| t >= 2.0);
    let shader = if stops.len() < 2 {
        None
    } else if radial {
        RadialGradient::new(a, 0.0, a, ((e.0 - s.0).powi(2) + (e.1 - s.1).powi(2)).sqrt().max(0.01), stops, SpreadMode::Pad, Transform::identity())
    } else {
        LinearGradient::new(a, b, stops, SpreadMode::Pad, Transform::identity())
    };
    match shader {
        Some(sh) => paint.shader = sh,
        None => paint.shader = Shader::SolidColor(colours.first().map_or(tiny_skia::Color::BLACK, |c| c.1)),
    }
    paint
}

impl Animation {
    /// Reads a Lottie file's text.
    pub fn parse(json: &str) -> Result<Animation, String> {
        if json.len() > MAX_BYTES {
            return Err("the animation file is too large".to_string());
        }
        let root: Value = serde_json::from_str(json).map_err(|e| format!("not a Lottie animation ({e})"))?;
        Animation::from_value(root)
    }

    pub fn from_value(root: Value) -> Result<Animation, String> {
        let n = |key: &str| root.get(key).and_then(f);
        let (Some(width), Some(height)) = (n("w"), n("h")) else { return Err("not a Lottie animation: it has no size".to_string()) };
        if !root.get("layers").is_some_and(Value::is_array) {
            return Err("not a Lottie animation: it has no layers".to_string());
        }
        if !(1.0..=MAX_SIDE).contains(&width) || !(1.0..=MAX_SIDE).contains(&height) {
            return Err("the animation's size isn't one that can be drawn".to_string());
        }
        let fps = n("fr").unwrap_or(30.0).clamp(1.0, 240.0);
        let first = n("ip").unwrap_or(0.0).clamp(-1.0e6, 1.0e6);
        let last = n("op").unwrap_or(first + fps).clamp(first + 1.0, first + fps * 3600.0);
        Ok(Animation { root, width, height, fps, first, last })
    }

    /// How long it runs (µs).
    pub fn duration_us(&self) -> u64 {
        (f64::from(self.last - self.first) / f64::from(self.fps) * 1e6) as u64
    }

    /// The text layers: each one's name and words.
    pub fn texts(&self) -> Vec<(String, String)> {
        items(self.root.get("layers").unwrap_or(&Value::Null), MAX_LAYERS)
            .iter()
            .filter(|l| l.get("ty").and_then(f) == Some(5.0))
            .map(|l| {
                let words = l.pointer("/t/d/k/0/s/t").and_then(Value::as_str).unwrap_or_default();
                (l.get("nm").and_then(Value::as_str).unwrap_or_default().to_string(), words.to_string())
            })
            .collect()
    }

    /// Changes the words of the text layers called `name` (whatever their case); whether there
    /// was one.
    pub fn set_text(&mut self, name: &str, words: &str) -> bool {
        let mut found = false;
        let Some(layers) = self.root.get_mut("layers").and_then(Value::as_array_mut) else { return false };
        for l in layers.iter_mut().take(MAX_LAYERS) {
            let named = l.get("nm").and_then(Value::as_str).is_some_and(|n| n.eq_ignore_ascii_case(name));
            if named
                && l.get("ty").and_then(f) == Some(5.0)
                && let Some(t) = l.pointer_mut("/t/d/k/0/s/t")
            {
                *t = Value::String(words.chars().take(400).collect());
                found = true;
            }
        }
        found
    }

    /// The frame shown `secs` in (its last one after its end).
    pub fn frame_at(&self, secs: f32) -> f32 {
        let fr = self.first + if secs.is_finite() { secs.max(0.0) } else { 0.0 } * self.fps;
        fr.clamp(self.first, self.last - 0.01)
    }

    /// Draws the frame `secs` in onto `pix`, the animation's own pixels placed by `place`.
    pub fn render(&self, pix: &mut Pixmap, secs: f32, place: Transform) {
        self.render_frame(pix, self.frame_at(secs), place);
    }

    /// Draws a frame (by its number, fractions too).
    pub fn render_frame(&self, pix: &mut Pixmap, frame: f32, place: Transform) {
        if !frame.is_finite() || !place.is_finite() {
            return;
        }
        self.layers(pix, self.root.get("layers").unwrap_or(&Value::Null), frame, place, 1.0, 0);
    }

    /// A layer's transform through its parents, and its own opacity.
    fn world(&self, list: &[Value], layer: &Value, frame: f32) -> (Transform, f32) {
        let (mut t, o) = xform(layer.get("ks"), frame);
        let mut parent = layer.get("parent").and_then(f);
        for _ in 0..16 {
            let Some(p) = parent else { break };
            let Some(pl) = list.iter().find(|l| l.get("ind").and_then(f) == Some(p)) else { break };
            t = t.post_concat(xform(pl.get("ks"), frame).0);
            parent = pl.get("parent").and_then(f);
        }
        (t, o)
    }

    fn layers(&self, pix: &mut Pixmap, layers: &Value, frame: f32, base: Transform, alpha: f32, depth: usize) {
        let list = items(layers, MAX_LAYERS);
        // The first layer is on top: drawn last.
        for (idx, l) in list.iter().enumerate().rev() {
            let num = |key: &str| l.get(key).and_then(f);
            let hidden = l.get("hd").and_then(Value::as_bool).unwrap_or(false);
            // A matte's source isn't drawn itself.
            if hidden || num("td").is_some_and(|v| v >= 1.0) {
                continue;
            }
            if frame < num("ip").unwrap_or(f32::MIN) || frame >= num("op").unwrap_or(f32::MAX) {
                continue;
            }
            let (world, o) = self.world(list, l, frame);
            let (world, o) = (world.post_concat(base), o * alpha);
            if o <= 0.0 || !world.is_finite() {
                continue;
            }
            let masks = items(l.get("masksProperties").unwrap_or(&Value::Null), 16);
            let matte = num("tt").filter(|t| *t >= 1.0).and_then(|t| idx.checked_sub(1).and_then(|j| list.get(j)).map(|src| (t, src)));
            if masks.is_empty() && matte.is_none() {
                self.layer(pix, l, frame, world, o, depth);
                continue;
            }
            // Drawn apart, cut by its masks and its matte, then put on.
            let Some(mut own) = Pixmap::new(pix.width(), pix.height()) else { continue };
            self.layer(&mut own, l, frame, world, 1.0, depth);
            let cut: Vec<&Value> = masks.iter().filter(|m| m.get("mode").and_then(Value::as_str) != Some("n")).collect();
            if !cut.is_empty()
                && let Some(mut mask) = Mask::new(pix.width(), pix.height())
            {
                for m in cut {
                    if let Some(path) = bezier(m.get("pt"), frame).and_then(|b| path_of(&b)) {
                        mask.fill_path(&path, FillRule::Winding, true, world);
                    }
                    if m.get("inv").and_then(Value::as_bool).unwrap_or(false) || m.get("mode").and_then(Value::as_str) == Some("s") {
                        mask.invert();
                    }
                }
                own.apply_mask(&mask);
            }
            if let Some((kind, src)) = matte
                && let Some(mut shape) = Pixmap::new(pix.width(), pix.height())
            {
                let (sw, _) = self.world(list, src, frame);
                self.layer(&mut shape, src, frame, sw.post_concat(base), 1.0, depth);
                let mut mask = Mask::from_pixmap(shape.as_ref(), MaskType::Alpha);
                if kind >= 2.0 {
                    mask.invert();
                }
                own.apply_mask(&mask);
            }
            pix.draw_pixmap(0, 0, own.as_ref(), &PixmapPaint { opacity: o, ..PixmapPaint::default() }, Transform::identity(), None);
        }
    }

    fn layer(&self, pix: &mut Pixmap, l: &Value, frame: f32, world: Transform, alpha: f32, depth: usize) {
        match l.get("ty").and_then(f).map(|t| t as i32) {
            Some(4) => self.group(pix, l.get("shapes").unwrap_or(&Value::Null), frame, world, alpha, 0),
            Some(1) => {
                let (w, h) = (l.get("sw").and_then(f).unwrap_or(0.0), l.get("sh").and_then(f).unwrap_or(0.0));
                let hex = l.get("sc").and_then(Value::as_str).unwrap_or("#000000").trim_start_matches('#');
                let c = |k: usize| hex.get(k * 2..k * 2 + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).map_or(0.0, |v| f32::from(v) / 255.0);
                let mut paint = Paint { anti_alias: true, ..Paint::default() };
                paint.set_color(colour(&[c(0), c(1), c(2)], alpha));
                if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, w.max(0.01), h.max(0.01)) {
                    pix.fill_rect(r, &paint, world, None);
                }
            }
            Some(0) if depth < MAX_DEPTH => {
                let id = l.get("refId").and_then(Value::as_str).unwrap_or_default();
                let asset =
                    items(self.root.get("assets").unwrap_or(&Value::Null), 200).iter().find(|a| a.get("id").and_then(Value::as_str) == Some(id));
                if let Some(layers) = asset.and_then(|a| a.get("layers")) {
                    let start = l.get("st").and_then(f).unwrap_or(0.0);
                    self.layers(pix, layers, frame - start, world, alpha, depth + 1);
                }
            }
            Some(5) => self.text(pix, l, world, alpha),
            _ => {}
        }
    }

    /// A text layer, in this program's own fonts: its origin is the start of its first line's
    /// baseline (its middle, or its end, as it's justified).
    fn text(&self, pix: &mut Pixmap, l: &Value, world: Transform, alpha: f32) {
        let Some(doc) = l.pointer("/t/d/k/0/s") else { return };
        let words = doc.get("t").and_then(Value::as_str).unwrap_or_default().replace('\r', "\n");
        let size = doc.get("s").and_then(f).unwrap_or(36.0).clamp(1.0, 2000.0);
        let font = doc.get("f").and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
        let bold = ["bold", "black", "heavy", "semibold", "demi"].iter().any(|w| font.contains(w));
        // Laid out at a fixed size and scaled, so any size is drawn.
        let style =
            screengrabcraft_model::Style { font_size: 64.0, bold, italic: font.contains("italic"), ..screengrabcraft_model::Style::default() };
        let mut lay = text::layout(&words, &style, None);
        lay.lines.truncate(12);
        let k = size / 64.0;
        let lead = doc.get("lh").and_then(f).filter(|v| *v > 0.0).map_or(lay.line_height, |lh| lh / k);
        lay.line_height = lead;
        let just = doc.get("j").and_then(f).unwrap_or(0.0) as i32;
        let mut pb = PathBuilder::new();
        for (n, line) in lay.lines.iter().enumerate() {
            let one_line = text::Layout { lines: vec![line.clone()], width: line.width, ..lay.clone() };
            let dx = match just {
                1 => -line.width,
                2 => -line.width / 2.0,
                _ => 0.0,
            };
            let origin = screengrabcraft_model::Pt::new(dx, -lay.ascent + lead * n as f32);
            if let Some(p) = text::text_path(&one_line, &style, origin, line.width) {
                pb.push_path(&p);
            }
        }
        let Some(path) = pb.finish() else { return };
        let mut paint = Paint { anti_alias: true, ..Paint::default() };
        paint.set_color(colour(&doc.get("fc").map(list).unwrap_or_else(|| vec![1.0, 1.0, 1.0]), alpha));
        pix.fill_path(&path, &paint, FillRule::Winding, Transform::from_scale(k, k).post_concat(world), None);
    }

    /// A shape layer's items, or a group's: its nested groups, then its own paths with each of
    /// its fills and strokes, from the bottom of the list up.
    fn group(&self, pix: &mut Pixmap, list: &Value, frame: f32, base: Transform, alpha: f32, depth: usize) {
        let all = items(list, MAX_ITEMS);
        let ty = |i: &Value| i.get("ty").and_then(Value::as_str).unwrap_or_default().to_string();
        let shown = |i: &&Value| !i.get("hd").and_then(Value::as_bool).unwrap_or(false);
        let (own, o) = xform(all.iter().find(|i| ty(i) == "tr"), frame);
        let (world, alpha) = (own.post_concat(base), alpha * o);
        if alpha <= 0.0 {
            return;
        }
        let mut paths: Vec<Path> = all.iter().filter(shown).filter_map(|i| shape_path(i, frame)).collect();
        if let Some(tm) = all.iter().filter(shown).find(|i| ty(i) == "tm") {
            let (s, e) = (one(tm.get("s"), frame, 0.0) / 100.0, one(tm.get("e"), frame, 100.0) / 100.0);
            let (s, e) = (s.min(e).clamp(0.0, 1.0), s.max(e).clamp(0.0, 1.0));
            let turn = (one(tm.get("o"), frame, 0.0) / 360.0).rem_euclid(1.0);
            let (s, e) = if s + turn >= 1.0 { (s + turn - 1.0, e + turn - 1.0) } else { (s + turn, e + turn) };
            paths = if e - s <= 1.0e-5 { Vec::new() } else { paths.iter().filter_map(|p| trimmed(p, s, e)).collect() };
        }
        for item in all.iter().rev().filter(shown) {
            match ty(item).as_str() {
                "gr" if depth < MAX_DEPTH => self.group(pix, item.get("it").unwrap_or(&Value::Null), frame, world, alpha, depth + 1),
                "fl" | "gf" => {
                    let rule = if item.get("r").and_then(f) == Some(2.0) { FillRule::EvenOdd } else { FillRule::Winding };
                    let paint = paint_of(item, frame, alpha);
                    for p in &paths {
                        pix.fill_path(p, &paint, rule, world, None);
                    }
                }
                "st" | "gs" => {
                    let width = one(item.get("w"), frame, 1.0).clamp(0.0, 2000.0);
                    if width <= 0.0 {
                        continue;
                    }
                    let cap = match item.get("lc").and_then(f).map(|v| v as i32) {
                        Some(2) => LineCap::Round,
                        Some(3) => LineCap::Square,
                        _ => LineCap::Butt,
                    };
                    let join = match item.get("lj").and_then(f).map(|v| v as i32) {
                        Some(2) => LineJoin::Round,
                        Some(3) => LineJoin::Bevel,
                        _ => LineJoin::Miter,
                    };
                    let stroke = Stroke { width, line_cap: cap, line_join: join, ..Stroke::default() };
                    let paint = paint_of(item, frame, alpha);
                    for p in &paths {
                        pix.stroke_path(p, &paint, &stroke, world, None);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn count(pix: &Pixmap, test: impl Fn(u8, u8, u8, u8) -> bool) -> usize {
        pix.pixels().iter().filter(|p| test(p.red(), p.green(), p.blue(), p.alpha())).count()
    }

    /// A red square sliding right over a second (eased), under a text layer, on 200 × 100.
    fn sample() -> Value {
        json!({"v": "5.7.0", "fr": 30, "ip": 0, "op": 60, "w": 200, "h": 100, "layers": [
            {"ty": 5, "nm": "Title", "ind": 1, "ip": 0, "op": 60, "ks": {"p": {"k": [100, 90]}},
             "t": {"d": {"k": [{"s": {"s": 24, "f": "Sans-Bold", "t": "Hi", "j": 2, "fc": [1, 1, 1]}, "t": 0}]}}},
            {"ty": 4, "nm": "Square", "ind": 2, "ip": 0, "op": 60,
             "ks": {"o": {"k": 100}, "p": {"a": 1, "k": [
                 {"t": 0, "s": [20, 30], "i": {"x": [0.2], "y": [1]}, "o": {"x": [0.4], "y": [0]}},
                 {"t": 30, "s": [120, 30]}]}},
             "shapes": [{"ty": "gr", "it": [
                 {"ty": "rc", "p": {"k": [0, 0]}, "s": {"k": [20, 20]}, "r": {"k": 0}},
                 {"ty": "fl", "c": {"k": [1, 0, 0, 1]}, "o": {"k": 100}},
                 {"ty": "tr", "p": {"k": [0, 0]}, "a": {"k": [0, 0]}, "s": {"k": [100, 100]}, "r": {"k": 0}, "o": {"k": 100}}]}]}
        ]})
    }

    fn red_at(a: &Animation, frame: f32) -> (usize, f32) {
        let mut pix = Pixmap::new(200, 100).unwrap();
        a.render_frame(&mut pix, frame, Transform::identity());
        let reds: Vec<usize> =
            pix.pixels().iter().enumerate().filter(|(_, p)| p.red() > 200 && p.green() < 50 && p.alpha() > 200).map(|(i, _)| i % 200).collect();
        (reds.len(), reds.iter().sum::<usize>() as f32 / reds.len().max(1) as f32)
    }

    #[test]
    fn a_shape_moves_along_its_keyframes_with_its_easing() {
        let a = Animation::from_value(sample()).unwrap();
        assert_eq!((a.width, a.height, a.fps, a.duration_us()), (200.0, 100.0, 30.0, 2_000_000));
        let (n, x0) = red_at(&a, 0.0);
        assert!((380..=420).contains(&n) && (x0 - 19.5).abs() < 1.0, "{n} {x0}");
        // Half-way in time it's past half-way there (it eases out), and it ends where it should.
        let (_, mid) = red_at(&a, 15.0);
        assert!(mid > 75.0 && mid < 119.0, "{mid}");
        let (_, end) = red_at(&a, 45.0);
        assert!((end - 119.5).abs() < 1.0, "{end}");
        // By time, and past its end its last frame.
        let mut pix = Pixmap::new(200, 100).unwrap();
        a.render(&mut pix, 99.0, Transform::identity());
        assert!(count(&pix, |r, g, _, al| r > 200 && g < 50 && al > 200) > 380);
        // Drawn twice the size.
        let mut big = Pixmap::new(400, 200).unwrap();
        a.render(&mut big, 0.0, Transform::from_scale(2.0, 2.0));
        assert!(count(&big, |r, g, _, al| r > 200 && g < 50 && al > 200) > 1500);
    }

    #[test]
    fn its_words_are_drawn_and_can_be_changed() {
        let mut a = Animation::from_value(sample()).unwrap();
        assert_eq!(a.texts(), vec![("Title".to_string(), "Hi".to_string())]);
        let white = |a: &Animation| {
            let mut pix = Pixmap::new(200, 100).unwrap();
            a.render_frame(&mut pix, 0.0, Transform::identity());
            count(&pix, |r, g, b, al| r > 200 && g > 200 && b > 200 && al > 200)
        };
        let short = white(&a);
        assert!(short > 30, "{short}");
        assert!(a.set_text("title", "Hello there") && !a.set_text("nothing", "x"));
        assert!(white(&a) > short * 2);
    }

    #[test]
    fn a_line_is_drawn_on_by_trimming_and_a_matte_cuts_a_layer() {
        let line = |end: f32| {
            json!({"fr": 30, "ip": 0, "op": 30, "w": 200, "h": 100, "layers": [{"ty": 4, "ip": 0, "op": 30, "ks": {}, "shapes": [
                {"ty": "sh", "ks": {"k": {"v": [[20, 50], [180, 50]], "i": [[0, 0], [0, 0]], "o": [[0, 0], [0, 0]], "c": false}}},
                {"ty": "tm", "s": {"k": 0}, "e": {"k": end}, "o": {"k": 0}},
                {"ty": "st", "c": {"k": [0, 1, 0]}, "o": {"k": 100}, "w": {"k": 6}}]}]})
        };
        let green = |v: Value| {
            let mut pix = Pixmap::new(200, 100).unwrap();
            Animation::from_value(v).unwrap().render_frame(&mut pix, 0.0, Transform::identity());
            count(&pix, |r, g, _, al| g > 200 && r < 50 && al > 200)
        };
        let (half, whole) = (green(line(50.0)), green(line(100.0)));
        assert!(whole > 800 && (half as f32 - whole as f32 / 2.0).abs() < 40.0, "{half} {whole}");
        assert_eq!(green(line(0.0)), 0);
        // A blue solid shown only through a circle (the layer above it, its matte).
        let matte = json!({"fr": 30, "ip": 0, "op": 30, "w": 200, "h": 100, "layers": [
            {"ty": 4, "td": 1, "ip": 0, "op": 30, "ks": {"p": {"k": [100, 50]}}, "shapes": [
                {"ty": "el", "p": {"k": [0, 0]}, "s": {"k": [40, 40]}}, {"ty": "fl", "c": {"k": [1, 1, 1]}, "o": {"k": 100}}]},
            {"ty": 1, "tt": 1, "ip": 0, "op": 30, "ks": {}, "sw": 200, "sh": 100, "sc": "#0000ff"}]});
        let mut pix = Pixmap::new(200, 100).unwrap();
        Animation::from_value(matte).unwrap().render_frame(&mut pix, 0.0, Transform::identity());
        let blue = count(&pix, |r, _, b, al| b > 200 && r < 50 && al > 200);
        assert!((1100..=1300).contains(&blue), "a circle of it: {blue}");
    }

    #[test]
    fn junk_is_refused_or_drawn_as_far_as_it_makes_sense() {
        for bad in ["", "[]", "{}", "{\"w\": 10}", "{\"w\": 1e99, \"h\": 5, \"layers\": []}", "{\"w\": 10, \"h\": 10, \"layers\": 5}", "not json"] {
            assert!(Animation::parse(bad).is_err(), "{bad}");
        }
        // Layers of every wrong kind, nested without end, numbers that aren't: drawn or left out.
        let mut nested = json!({"ty": "gr", "it": []});
        for _ in 0..40 {
            nested = json!({"ty": "gr", "it": [nested, {"ty": "fl", "c": {"k": "red"}}, {"ty": "sh", "ks": {"k": {"v": "x"}}}]});
        }
        let junk = json!({"w": 64, "h": 64, "fr": 0, "ip": 5, "op": -3, "assets": [{"id": "a", "layers": [{"ty": 0, "refId": "a", "ks": {}}]}], "layers": [
            {"ty": 4, "shapes": [nested], "ks": {"p": {"a": 1, "k": [{"t": "soon"}, {"t": null, "s": {}}]}, "s": {"k": [null, 1e300]}}},
            {"ty": 0, "refId": "a", "parent": 9, "ind": 9, "ks": {"o": {"k": []}}},
            {"ty": 5, "t": {"d": {"k": [{"s": {"s": -5, "t": 7}}]}}},
            {"ty": 5, "t": {"d": {"k": [{"s": {"s": 1e9, "t": "big", "lh": 0}}]}}, "masksProperties": [{"pt": {"k": 4}}, {"mode": "s"}]},
            {"ty": 1, "tt": 2, "sw": -4, "sh": "tall", "sc": "#zz"}, {"ty": 99}, 17, null,
            {"ty": 4, "shapes": [{"ty": "el", "s": {"k": [0, 0]}}, {"ty": "rc", "s": {"k": [1e30, 1e30]}, "r": {"k": -9}}, {"ty": "tm", "s": {"k": 900}, "e": {"k": -900}, "o": {"k": 1e9}},
                {"ty": "gf", "g": {"p": 9999, "k": {"k": [0, 1]}}}, {"ty": "st", "w": {"k": 1e9}, "c": {"k": []}}]}]});
        let a = Animation::from_value(junk).unwrap();
        let mut pix = Pixmap::new(64, 64).unwrap();
        for frame in [0.0, 5.0, f32::NAN, f32::INFINITY, -1.0e30] {
            a.render_frame(&mut pix, frame, Transform::identity());
            a.render(&mut pix, frame, Transform::from_scale(f32::NAN, 1.0));
        }
        assert!(a.duration_us() > 0);
    }
}
