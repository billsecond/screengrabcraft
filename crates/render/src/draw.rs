//! Drawing a document: the picture, then each annotation in order. Annotations that change the
//! pixels under them (blur, pixelate, spotlight, magnify) work on what has been drawn so far, so
//! a blur hides an arrow drawn before it and not one drawn after.

use screengrabcraft_model::object::{PIN_POINT, arrow_head, bent_points, polygon_points, step_radius};
use screengrabcraft_model::{ArrowHeads, BadgeShape, CalloutForm, Color, Document, HeadShape, Object, ObjectId, Pt, Raster, Rect, Shape, Style};
use tiny_skia::{
    BlendMode, FillRule, FilterQuality, LineCap, LineJoin, Paint, Path, PathBuilder, Pattern, Pixmap, PixmapPaint, SpreadMode, Stroke, StrokeDash,
    Transform,
};

use crate::canvas;
use crate::text;

/// What to leave out of a rendering.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Annotations not drawn.
    pub hide: Vec<ObjectId>,
    /// An annotation drawn without its text (the one whose text is being typed).
    pub no_text: Option<ObjectId>,
}

/// Renders documents, keeping the picture's pixmap between renderings of the same picture.
#[derive(Default)]
pub struct Renderer {
    base: Option<(Raster, Pixmap)>,
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer::default()
    }

    /// The document drawn: its picture with every annotation on it.
    pub fn render(&mut self, doc: &Document, opts: &Options) -> Option<Pixmap> {
        let cached = self.base.as_ref().is_some_and(|(r, _)| r.same_pixels(&doc.base) && r.width() == doc.width());
        if !cached {
            self.base = Some((doc.base.clone(), canvas::to_pixmap(&doc.base)?));
        }
        let mut out = self.base.as_ref()?.1.clone();
        for o in &doc.objects {
            if opts.hide.contains(&o.id) {
                continue;
            }
            if opts.no_text == Some(o.id) {
                let mut blank = o.clone();
                if let Some(t) = blank.shape.text_mut() {
                    t.clear();
                }
                draw_object(&mut out, doc, &blank);
            } else {
                draw_object(&mut out, doc, o);
            }
        }
        Some(out)
    }
}

/// The document flattened into one picture.
pub fn render(doc: &Document) -> Option<Raster> {
    let pix = Renderer::new().render(doc, &Options::default())?;
    canvas::from_pixmap(&pix)
}

pub fn skia_color(c: Color, opacity: f32) -> tiny_skia::Color {
    let a = (f32::from(c.a) / 255.0) * opacity.clamp(0.0, 1.0);
    tiny_skia::Color::from_rgba(f32::from(c.r) / 255.0, f32::from(c.g) / 255.0, f32::from(c.b) / 255.0, a.clamp(0.0, 1.0))
        .unwrap_or(tiny_skia::Color::BLACK)
}

fn paint(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(skia_color(c, 1.0));
    p.anti_alias = true;
    p
}

fn stroke_of(width: f32, dashed: bool, cap: LineCap) -> Stroke {
    let width = width.max(0.5);
    let mut s = Stroke { width, line_cap: cap, line_join: LineJoin::Round, ..Stroke::default() };
    if dashed {
        s.dash = StrokeDash::new(vec![width * 2.5, width * 2.0], 0.0);
        s.line_cap = LineCap::Butt;
    }
    s
}

fn visible(c: Color) -> bool {
    c.a > 0
}

/// A rectangle with rounded corners (`r` clamped to fit).
pub fn rounded_rect(r: Rect, radius: f32) -> Option<Path> {
    let rad = radius.max(0.0).min(r.w / 2.0).min(r.h / 2.0);
    if rad <= 0.5 {
        return PathBuilder::from_rect(tiny_skia::Rect::from_xywh(r.x, r.y, r.w.max(0.1), r.h.max(0.1))?).into();
    }
    // A quarter circle as a cubic: control points at k·radius.
    let k = 0.552_284_8 * rad;
    let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + rad, y0);
    pb.line_to(x1 - rad, y0);
    pb.cubic_to(x1 - rad + k, y0, x1, y0 + rad - k, x1, y0 + rad);
    pb.line_to(x1, y1 - rad);
    pb.cubic_to(x1, y1 - rad + k, x1 - rad + k, y1, x1 - rad, y1);
    pb.line_to(x0 + rad, y1);
    pb.cubic_to(x0 + rad - k, y1, x0, y1 - rad + k, x0, y1 - rad);
    pb.line_to(x0, y0 + rad);
    pb.cubic_to(x0, y0 + rad - k, x0 + rad - k, y0, x0 + rad, y0);
    pb.close();
    pb.finish()
}

fn oval(r: Rect) -> Option<Path> {
    PathBuilder::from_oval(tiny_skia::Rect::from_xywh(r.x, r.y, r.w.max(0.5), r.h.max(0.5))?)
}

fn circle(c: Pt, r: f32) -> Option<Path> {
    PathBuilder::from_circle(c.x, c.y, r.max(0.5))
}

/// A smooth line through freehand points (quadratic curves through the midpoints).
fn smooth(points: &[Pt]) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let first = points.first()?;
    pb.move_to(first.x, first.y);
    if points.len() == 1 {
        pb.line_to(first.x + 0.01, first.y);
        return pb.finish();
    }
    if points.len() == 2 {
        let p = points.get(1)?;
        pb.line_to(p.x, p.y);
        return pb.finish();
    }
    for w in points.windows(2).skip(1) {
        if let [a, b] = w {
            let mid = a.lerp(*b, 0.5);
            pb.quad_to(a.x, a.y, mid.x, mid.y);
        }
    }
    let last = points.last()?;
    pb.line_to(last.x, last.y);
    pb.finish()
}

/// The shadow under annotations: how far it falls and how soft it is.
fn shadow_geometry(style: &Style) -> (f32, f32) {
    let w = style.width.max(1.0);
    ((2.0 + w * 0.35).min(10.0), (3.0 + w * 0.45).min(14.0))
}

/// Draws into a layer the size of `bounds` (plus room for the shadow), then puts the layer on
/// the canvas with the style's shadow and opacity, and `blend`.
fn layered(canvas: &mut Pixmap, bounds: Rect, style: &Style, blend: BlendMode, layer_alpha: f32, draw: &dyn Fn(&mut Pixmap, Transform)) {
    let (offset, soft) = shadow_geometry(style);
    let margin = if style.shadow { soft * 3.0 + offset + 2.0 } else { 2.0 };
    let full = Rect::new(0.0, 0.0, canvas.width() as f32, canvas.height() as f32);
    let Some(area) = bounds.inflate(margin).intersect(full) else { return };
    let (lx, ly) = (area.x.floor(), area.y.floor());
    let (lw, lh) = ((area.right().ceil() - lx).max(1.0) as u32, (area.bottom().ceil() - ly).max(1.0) as u32);
    let Some(mut layer) = canvas::blank(lw, lh) else { return };
    draw(&mut layer, Transform::from_translate(-lx, -ly));
    if style.shadow
        && let Some(mut shadow) = canvas::blank(lw, lh)
    {
        // The layer's coverage, in a soft dark grey.
        for (s, l) in shadow.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(layer.data().as_chunks::<4>().0.iter()) {
            let a = (u16::from(l[3]) * 110 / 255) as u8;
            s.copy_from_slice(&[0, 0, 0, a]);
        }
        canvas::blur(&mut shadow, soft);
        let pp =
            PixmapPaint { opacity: style.opacity.clamp(0.0, 1.0) * layer_alpha, blend_mode: BlendMode::SourceOver, quality: FilterQuality::Nearest };
        canvas.draw_pixmap((lx + offset) as i32, (ly + offset) as i32, shadow.as_ref(), &pp, Transform::identity(), None);
    }
    let pp = PixmapPaint { opacity: (style.opacity * layer_alpha).clamp(0.0, 1.0), blend_mode: blend, quality: FilterQuality::Nearest };
    canvas.draw_pixmap(lx as i32, ly as i32, layer.as_ref(), &pp, Transform::identity(), None);
}

/// Whether drawing straight onto the canvas gives the same result as a layer.
fn direct(style: &Style) -> bool {
    !style.shadow && style.opacity >= 0.999
}

fn with_style(canvas: &mut Pixmap, bounds: Rect, style: &Style, draw: &dyn Fn(&mut Pixmap, Transform)) {
    if direct(style) {
        draw(canvas, Transform::identity());
    } else {
        layered(canvas, bounds, style, BlendMode::SourceOver, 1.0, draw);
    }
}

/// How far round a turned annotation its layer reaches, for its shadow.
const TURNED_MARGIN: f32 = 64.0;

/// A turned annotation: drawn square on a layer of its own, then put on the canvas turned round
/// its middle (only the part that can land on the canvas is drawn).
fn draw_turned(canvas: &mut Pixmap, doc: &Document, o: &Object, turn: f32) {
    let style = o.style.clamped();
    let c = o.shape.pivot();
    let full = Rect::new(0.0, 0.0, canvas.width() as f32, canvas.height() as f32);
    let reach = screengrabcraft_model::object::turned_box(full, c, -turn);
    let Some(area) = o.shape.bounds(&style).inflate(TURNED_MARGIN).intersect(reach) else { return };
    let (x, y) = (area.x.floor(), area.y.floor());
    let (w, h) = ((area.right().ceil() - x).max(1.0) as u32, (area.bottom().ceil() - y).max(1.0) as u32);
    let Some(mut layer) = canvas::blank(w, h) else { return };
    let mut square = o.clone();
    square.rotation = 0.0;
    square.shape.translate(Pt::new(-x, -y));
    draw_object(&mut layer, doc, &square);
    // A highlight multiplies into what is under it, turned or not.
    let blend = if o.shape.kind() == screengrabcraft_model::Kind::Highlight { BlendMode::Multiply } else { BlendMode::SourceOver };
    let paint = PixmapPaint { opacity: 1.0, blend_mode: blend, quality: FilterQuality::Bilinear };
    canvas.draw_pixmap(0, 0, layer.as_ref(), &paint, Transform::from_rotate_at(turn, c.x, c.y).pre_translate(x, y), None);
}

/// Draws one annotation onto the canvas.
pub fn draw_object(canvas: &mut Pixmap, doc: &Document, o: &Object) {
    let turn = o.turn();
    if turn != 0.0 {
        draw_turned(canvas, doc, o, turn);
        return;
    }
    let style = o.style.clamped();
    let bounds = o.shape.bounds(&style);
    match &o.shape {
        Shape::Arrow { from, to, heads, bend, head } => {
            let (from, to, heads, bend, head) = (*from, *to, *heads, *bend, *head);
            with_style(canvas, bounds, &style, &|c, t| draw_arrow(c, t, (from, to, bend), heads, head, &style));
        }
        Shape::Line { from, to } => {
            let (from, to) = (*from, *to);
            with_style(canvas, bounds, &style, &|c, t| {
                let mut pb = PathBuilder::new();
                pb.move_to(from.x, from.y);
                pb.line_to(to.x, to.y);
                if let Some(path) = pb.finish() {
                    c.stroke_path(&path, &paint(style.stroke), &stroke_of(style.width, style.dashed, LineCap::Round), t, None);
                }
            });
        }
        Shape::Rect { rect, radius } => {
            let (rect, radius) = (*rect, *radius);
            with_style(canvas, bounds, &style, &|c, t| {
                if let Some(path) = rounded_rect(rect, radius) {
                    fill_and_stroke(c, &path, &style, t);
                }
            });
        }
        Shape::Ellipse { rect } => {
            let rect = *rect;
            with_style(canvas, bounds, &style, &|c, t| {
                if let Some(path) = oval(rect) {
                    fill_and_stroke(c, &path, &style, t);
                }
            });
        }
        Shape::Polygon { rect, sides, star } => {
            let corners = polygon_points(*rect, *sides, *star);
            with_style(canvas, bounds, &style, &|c, t| {
                if let Some(path) = polygon(&corners) {
                    fill_and_stroke(c, &path, &style, t);
                }
            });
        }
        Shape::Pen { points } => {
            with_style(canvas, bounds, &style, &|c, t| {
                if let Some(path) = smooth(points) {
                    c.stroke_path(&path, &paint(style.stroke), &stroke_of(style.width, style.dashed, LineCap::Round), t, None);
                }
            });
        }
        Shape::Highlight { rect, .. } => {
            // A box drawn opaque in a layer, then multiplied in at the colour's own transparency:
            // the picture under it shows through, tinted, as under a marker.
            let rect = *rect;
            let alpha = f32::from(style.stroke.a) / 255.0;
            let opaque = style.stroke.with_alpha(255);
            let plain = Style { shadow: false, ..style };
            layered(canvas, bounds, &plain, BlendMode::Multiply, alpha, &|c, t| {
                if let Some(path) = rounded_rect(rect, 0.0) {
                    c.fill_path(&path, &paint(opaque), FillRule::Winding, t, None);
                }
            });
        }
        Shape::Text { rect, text: body } => {
            let rect = *rect;
            with_style(canvas, bounds, &style, &|c, t| draw_text_box(c, t, rect, body, &style));
        }
        Shape::Callout { rect, tail, text: body, form } => {
            let (rect, tail, form) = (*rect, *tail, *form);
            with_style(canvas, bounds, &style, &|c, t| {
                for path in callout_paths(rect, tail, form, (style.font_size * 0.6).clamp(4.0, 24.0)) {
                    fill_and_stroke(c, &path, &style, t);
                }
                draw_text_centered(c, t, form.text_area(rect), body, &style);
            });
        }
        Shape::Step { center, number, labels, badge } => {
            let (center, badge) = (*center, *badge);
            let label = labels.text(*number);
            with_style(canvas, bounds, &style, &|c, t| draw_step(c, t, center, &label, badge, &style));
        }
        Shape::Blur { rect } => {
            if let Some((x, y, w, h)) = rect.pixel_bounds(canvas.width(), canvas.height()) {
                canvas::blur_area(canvas, x, y, w, h, style.strength.max(1.0));
            }
        }
        Shape::Pixelate { rect } => {
            if let Some((x, y, w, h)) = rect.pixel_bounds(canvas.width(), canvas.height()) {
                canvas::pixelate_area(canvas, x, y, w, h, style.strength.round().max(2.0) as u32);
            }
        }
        Shape::Spotlight { rect, ellipse } => draw_spotlight(canvas, *rect, *ellipse, &style),
        Shape::Magnify { rect } => draw_magnify(canvas, *rect, &style),
        Shape::Stamp { rect, stamp } => {
            let (rect, stamp) = (*rect, *stamp);
            with_style(canvas, bounds, &style, &|c, t| crate::stamp::draw(c, t, rect, stamp, &style));
        }
        Shape::Image { rect, image } => {
            let Some(pic) = doc.images.get(image).and_then(canvas::to_pixmap) else { return };
            let rect = *rect;
            with_style(canvas, bounds, &style, &|c, t| {
                let (sx, sy) = (rect.w / pic.width().max(1) as f32, rect.h / pic.height().max(1) as f32);
                let place = t.pre_translate(rect.x, rect.y).pre_scale(sx, sy);
                let pp = PixmapPaint { opacity: 1.0, blend_mode: BlendMode::SourceOver, quality: FilterQuality::Bicubic };
                c.draw_pixmap(0, 0, pic.as_ref(), &pp, place, None);
                if style.width > 0.0
                    && visible(style.stroke)
                    && let Some(path) = rounded_rect(rect, 0.0)
                {
                    c.stroke_path(&path, &paint(style.stroke), &stroke_of(style.width, style.dashed, LineCap::Square), t, None);
                }
            });
        }
    }
}

fn fill_and_stroke(c: &mut Pixmap, path: &Path, style: &Style, t: Transform) {
    if visible(style.fill) {
        c.fill_path(path, &paint(style.fill), FillRule::Winding, t, None);
    }
    if style.width > 0.0 && visible(style.stroke) {
        c.stroke_path(path, &paint(style.stroke), &stroke_of(style.width, style.dashed, LineCap::Round), t, None);
    }
}

/// The point `dist` along a line through `points` from its start, and the index of the piece
/// it's on; the end when the line is shorter.
fn along(points: &[Pt], dist: f32) -> (Pt, usize) {
    let mut left = dist.max(0.0);
    for (i, w) in points.windows(2).enumerate() {
        if let [a, b] = w {
            let l = a.distance(*b);
            if l >= left && l > 0.0 {
                return (a.lerp(*b, left / l), i);
            }
            left -= l;
        }
    }
    (points.last().copied().unwrap_or(Pt::ZERO), points.len().saturating_sub(2))
}

/// The line through `points` with `start` taken off its start and `end` off its end.
fn trimmed(points: &[Pt], start: f32, end: f32) -> Vec<Pt> {
    let total: f32 = points.windows(2).map(|w| if let [a, b] = w { a.distance(*b) } else { 0.0 }).sum();
    if total <= start + end {
        let (p, _) = along(points, total / 2.0);
        return vec![p, p];
    }
    let (a, ia) = along(points, start);
    let (b, ib) = along(points, total - end);
    let mut out = vec![a];
    out.extend(points.iter().skip(ia + 1).take(ib.saturating_sub(ia)));
    out.push(b);
    out
}

fn line_length(points: &[Pt]) -> f32 {
    points.windows(2).map(|w| if let [a, b] = w { a.distance(*b) } else { 0.0 }).sum()
}

/// An arrow along `line` (from, to, bend), with `heads` shaped `shape`.
fn draw_arrow(c: &mut Pixmap, t: Transform, line: (Pt, Pt, f32), heads: ArrowHeads, shape: HeadShape, style: &Style) {
    let (from, to, bend) = line;
    let points = bent_points(from, to, bend, 64);
    let len = line_length(&points);
    let ink = paint(style.stroke);
    if len < 0.5 {
        if let Some(dot) = circle(from, style.width / 2.0) {
            c.fill_path(&dot, &ink, FillRule::Winding, t, None);
        }
        return;
    }
    let (mut head_len, mut head_half) = arrow_head(style.width);
    let n_heads = match heads {
        ArrowHeads::None => 0.0,
        ArrowHeads::Both => 2.0,
        _ => 1.0,
    };
    // A short arrow gets smaller heads, so they don't overlap.
    if n_heads > 0.0 && head_len * n_heads > len * 0.8 {
        let f = len * 0.8 / (head_len * n_heads);
        head_len *= f;
        head_half *= f;
    }
    let start_head = matches!(heads, ArrowHeads::Start | ArrowHeads::Both);
    let end_head = matches!(heads, ArrowHeads::End | ArrowHeads::Both);
    // How much of the line a head covers: the shaft stops under it.
    let under = match shape {
        HeadShape::Notched | HeadShape::Triangle => head_len * 0.7,
        HeadShape::Open => style.width * 0.5,
        HeadShape::Dot => head_half * 0.5,
        HeadShape::Bar => 0.0,
    };
    let shaft = trimmed(&points, if start_head { under } else { 0.0 }, if end_head { under } else { 0.0 });
    if let Some(path) = open_line(&shaft) {
        let cap = if heads == ArrowHeads::None || shape == HeadShape::Open { LineCap::Round } else { LineCap::Butt };
        c.stroke_path(&path, &ink, &stroke_of(style.width, style.dashed, cap), t, None);
    }
    // Each head points the way the line runs over its own length.
    let head = |c: &mut Pixmap, tip: Pt, back: Pt| {
        let d = tip.sub(back);
        let l = d.length();
        if l <= 0.0 {
            return;
        }
        let dir = d.scale(1.0 / l);
        let normal = Pt::new(-dir.y, dir.x);
        let base = tip.sub(dir.scale(head_len));
        let (left, right) = (base.add(normal.scale(head_half)), base.sub(normal.scale(head_half)));
        let fill = |c: &mut Pixmap, path: Option<Path>| {
            if let Some(p) = path {
                c.fill_path(&p, &ink, FillRule::Winding, t, None);
            }
        };
        let line = |c: &mut Pixmap, pts: &[Pt]| {
            if let Some(p) = open_line(pts) {
                let s = Stroke { width: style.width.max(1.0), line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
                c.stroke_path(&p, &ink, &s, t, None);
            }
        };
        match shape {
            HeadShape::Notched => fill(c, polygon(&[tip, left, tip.sub(dir.scale(head_len * 0.78)), right])),
            HeadShape::Triangle => fill(c, polygon(&[tip, left, right])),
            // The lines' round ends reach past their points: the point is drawn that much back.
            HeadShape::Open => {
                let w = style.width.max(1.0) * 0.5;
                let back = dir.scale(w);
                line(c, &[left.sub(back), tip.sub(back), right.sub(back)]);
            }
            HeadShape::Dot => fill(c, circle(tip.sub(dir.scale(head_half * 0.6)), head_half * 0.6)),
            HeadShape::Bar => line(c, &[tip.add(normal.scale(head_half)), tip.sub(normal.scale(head_half))]),
        }
    };
    if end_head {
        let (back, _) = along(&points, len - head_len);
        head(c, to, back);
    }
    if start_head {
        let (back, _) = along(&points, head_len);
        head(c, from, back);
    }
}

/// An open line through `points`.
fn open_line(points: &[Pt]) -> Option<Path> {
    let (first, rest) = points.split_first()?;
    let mut pb = PathBuilder::new();
    pb.move_to(first.x, first.y);
    if rest.is_empty() {
        pb.line_to(first.x + 0.01, first.y);
    }
    for p in rest {
        pb.line_to(p.x, p.y);
    }
    pb.finish()
}

fn draw_text_box(c: &mut Pixmap, t: Transform, rect: Rect, body: &str, style: &Style) {
    let pad = text::padding(style);
    if (visible(style.fill) || (style.width > 0.0 && visible(style.stroke)))
        && let Some(bg) = rounded_rect(rect, pad.min(10.0))
    {
        fill_and_stroke(c, &bg, style, t);
    }
    let lay = text::layout(body, style, Some(rect.w - 2.0 * pad));
    if let Some(path) = text::text_path(&lay, style, Pt::new(rect.x + pad, rect.y + pad), rect.w - 2.0 * pad) {
        fill_text(c, &path, style, t);
    }
}

/// Text's letters: their outline (when the style has one) round them, then the letters over it.
fn fill_text(c: &mut Pixmap, path: &Path, style: &Style, t: Transform) {
    if style.has_text_outline() {
        // A stroke is centred on the letter's edge: twice as wide, half of it under the letter.
        let s = Stroke { width: style.text_outline_width * 2.0, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
        c.stroke_path(path, &paint(style.text_outline), &s, t, None);
    }
    c.fill_path(path, &paint(style.text_color), FillRule::Winding, t, None);
}

fn draw_text_centered(c: &mut Pixmap, t: Transform, rect: Rect, body: &str, style: &Style) {
    let pad = text::padding(style);
    let lay = text::layout(body, style, Some(rect.w - 2.0 * pad));
    let top = rect.y + ((rect.h - lay.height) / 2.0).max(pad);
    if let Some(path) = text::text_path(&lay, style, Pt::new(rect.x + pad, top), rect.w - 2.0 * pad) {
        fill_text(c, &path, style, t);
    }
}

/// A glyph string's path centred on a point.
pub fn centered_label(label: &str, style: &Style, center: Pt) -> Option<Path> {
    let lay = text::layout(label, style, None);
    let path = text::text_path(&lay, style, Pt::ZERO, lay.width)?;
    let b = path.bounds();
    let (cx, cy) = ((b.left() + b.right()) / 2.0, (b.top() + b.bottom()) / 2.0);
    path.transform(Transform::from_translate(center.x - cx, center.y - cy))
}

/// A closed outline through `points`.
fn polygon(points: &[Pt]) -> Option<Path> {
    let (first, rest) = points.split_first()?;
    let mut pb = PathBuilder::new();
    pb.move_to(first.x, first.y);
    for p in rest {
        pb.line_to(p.x, p.y);
    }
    pb.close();
    pb.finish()
}

/// The outline of a step badge of radius `r` round `center`.
pub fn badge_path(badge: BadgeShape, center: Pt, r: f32) -> Option<Path> {
    let r = r.max(0.5);
    let (side, up, _) = badge.reach(r);
    let (x, y) = (center.x, center.y);
    match badge {
        BadgeShape::Circle => circle(center, r),
        BadgeShape::Rounded => rounded_rect(Rect::new(x - side, y - up, 2.0 * side, 2.0 * up), r * 0.42),
        BadgeShape::Square => rounded_rect(Rect::new(x - side, y - up, 2.0 * side, 2.0 * up), 0.0),
        BadgeShape::Diamond => polygon(&[Pt::new(x, y - up), Pt::new(x + side, y), Pt::new(x, y + up), Pt::new(x - side, y)]),
        BadgeShape::Hexagon => {
            let corners: Vec<Pt> = (0..6)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::FRAC_PI_3;
                    Pt::new(x + up * a.cos(), y + up * a.sin())
                })
                .collect();
            polygon(&corners)
        }
        BadgeShape::Pin => {
            // A round head, its sides running down into the point where they meet the circle
            // at a tangent.
            let tip = Pt::new(x, y + r * PIN_POINT);
            let d = PIN_POINT * r;
            let half = (r / d).clamp(-1.0, 1.0).acos();
            let down = std::f32::consts::FRAC_PI_2;
            let (from, to) = (down + half, down - half + std::f32::consts::TAU);
            let steps = 40;
            let mut points = Vec::with_capacity(steps + 2);
            points.push(tip);
            for i in 0..=steps {
                let a = from + (to - from) * i as f32 / steps as f32;
                points.push(Pt::new(x + r * a.cos(), y + r * a.sin()));
            }
            polygon(&points)
        }
    }
}

fn draw_step(c: &mut Pixmap, t: Transform, center: Pt, label: &str, badge: BadgeShape, style: &Style) {
    let r = step_radius(style.font_size);
    let Some(outline) = badge_path(badge, center, r) else { return };
    c.fill_path(&outline, &paint(if visible(style.fill) { style.fill } else { Color::RED }), FillRule::Winding, t, None);
    if style.width > 0.0 && visible(style.stroke) {
        c.stroke_path(&outline, &paint(style.stroke), &stroke_of(style.width, false, LineCap::Round), t, None);
    }
    // Three figures or more are smaller; a label still too wide for the badge (VIII, AB) is
    // made to fit across it.
    let size = if label.chars().count() > 2 { style.font_size * 0.7 } else { style.font_size };
    let mut s = Style { font_size: size, bold: true, mono: false, italic: false, ..*style };
    let wide = text::layout(label, &s, None).width;
    let room = badge.label_room(r);
    if wide > room && wide > 0.0 {
        s.font_size = (size * room / wide).max(size * 0.35);
    }
    let Some(mut path) = centered_label(label, &s, center) else { return };
    // Smaller than the smallest letters (a count in the thousands in Roman numerals): the
    // letters themselves are made smaller.
    let w = path.bounds().width();
    if w > room && w > 0.0 {
        let f = room / w;
        let squeeze = Transform::from_translate(center.x, center.y).pre_scale(f, f).pre_translate(-center.x, -center.y);
        match path.clone().transform(squeeze) {
            Some(p) => path = p,
            None => return,
        }
    }
    c.fill_path(&path, &paint(style.text_color), FillRule::Winding, t, None);
}

/// The outlines of a callout of any form, in drawing order: one for most, the cloud and then
/// each of the bubbles of its tail for a cloud.
pub fn callout_paths(rect: Rect, tail: Pt, form: CalloutForm, radius: f32) -> Vec<Path> {
    match form {
        CalloutForm::Rounded => callout_path(rect, tail, radius).into_iter().collect(),
        CalloutForm::Square => callout_path(rect, tail, 0.0).into_iter().collect(),
        CalloutForm::Oval => oval_callout_path(rect, tail).into_iter().collect(),
        CalloutForm::Cloud => cloud_paths(rect, tail, form.bulge(rect)),
        CalloutForm::Arrow => arrow_callout_path(rect, tail).into_iter().collect(),
    }
}

/// Which side of `rect` faces `p` (0 top, 1 right, 2 bottom, 3 left): the direction from the
/// middle compared with the box's diagonals.
fn facing_side(rect: Rect, p: Pt) -> u8 {
    let d = p.sub(rect.center());
    if d.x.abs() * rect.h > d.y.abs() * rect.w {
        if d.x > 0.0 { 1 } else { 3 }
    } else if d.y > 0.0 {
        2
    } else {
        0
    }
}

/// A box whose side facing `tail` narrows to a point at `tail`: an arrow with room for text.
pub fn arrow_callout_path(rect: Rect, tail: Pt) -> Option<Path> {
    if rect.is_empty() || rect.inflate(-1.0).contains(tail) {
        return rounded_rect(rect, 0.0);
    }
    let (x0, y0, x1, y1) = (rect.x, rect.y, rect.right(), rect.bottom());
    let corners = [Pt::new(x0, y0), Pt::new(x1, y0), Pt::new(x1, y1), Pt::new(x0, y1)];
    // The point goes between the two corners of the side it leaves from.
    let side = usize::from(facing_side(rect, tail));
    let mut pb = PathBuilder::new();
    for (i, p) in corners.iter().enumerate() {
        if i == 0 {
            pb.move_to(p.x, p.y);
        } else {
            pb.line_to(p.x, p.y);
        }
        if i == side {
            pb.line_to(tail.x, tail.y);
        }
    }
    pb.close();
    pb.finish()
}

/// A point on the ellipse in `rect` at angle `a` (measured as on a circle the ellipse is
/// stretched from).
fn on_ellipse(c: Pt, rx: f32, ry: f32, a: f32) -> Pt {
    Pt::new(c.x + rx * a.cos(), c.y + ry * a.sin())
}

/// How many straight pieces draw an ellipse this big smoothly.
fn ellipse_steps(rx: f32, ry: f32) -> usize {
    let around = std::f32::consts::PI * (rx + ry);
    (around / 4.0).clamp(48.0, 720.0) as usize
}

/// An oval with a tail from its edge to `tail`, as one outline.
pub fn oval_callout_path(rect: Rect, tail: Pt) -> Option<Path> {
    let c = rect.center();
    let (rx, ry) = (rect.w / 2.0, rect.h / 2.0);
    let (nx, ny) = if rx >= 1.0 && ry >= 1.0 { ((tail.x - c.x) / rx, (tail.y - c.y) / ry) } else { (0.0, 0.0) };
    if nx * nx + ny * ny <= 1.0 || !(nx.is_finite() && ny.is_finite()) {
        return oval(rect);
    }
    // Round the ellipse the long way, from one side of the tail's base to the other, then out
    // to the tail and back.
    let a = ny.atan2(nx);
    let half = 0.24;
    let steps = ellipse_steps(rx, ry);
    let sweep = std::f32::consts::TAU - 2.0 * half;
    let mut pb = PathBuilder::new();
    let first = on_ellipse(c, rx, ry, a + half);
    pb.move_to(first.x, first.y);
    for i in 1..=steps {
        let p = on_ellipse(c, rx, ry, a + half + sweep * i as f32 / steps as f32);
        pb.line_to(p.x, p.y);
    }
    pb.line_to(tail.x, tail.y);
    pb.close();
    pb.finish()
}

/// A cloud: bumps round the ellipse in `rect`, reaching `bulge` out of it; then, when `tail` is
/// outside it, three bubbles getting smaller on the way to `tail`.
pub fn cloud_paths(rect: Rect, tail: Pt, bulge: f32) -> Vec<Path> {
    let c = rect.center();
    let (rx, ry) = (rect.w / 2.0, rect.h / 2.0);
    if rx < 2.0 || ry < 2.0 || !(c.x.is_finite() && c.y.is_finite()) {
        return oval(rect).into_iter().collect();
    }
    let bulge = bulge.max(1.0);
    let around = std::f32::consts::PI * (rx + ry);
    let n = (around / (bulge * 3.4)).round().clamp(8.0, 40.0) as usize;
    let mut out = Vec::new();
    let mut pb = PathBuilder::new();
    let point = |i: usize| on_ellipse(c, rx, ry, std::f32::consts::TAU * i as f32 / n as f32 - std::f32::consts::FRAC_PI_2);
    let start = point(0);
    pb.move_to(start.x, start.y);
    for i in 0..n {
        let (a, b) = (point(i), point(i + 1));
        let mid = a.lerp(b, 0.5);
        let out_dir = mid.sub(c);
        let len = out_dir.length().max(0.001);
        // A quadratic's middle is halfway to its control point: twice the bulge out.
        let ctrl = mid.add(out_dir.scale(2.0 * bulge / len));
        pb.quad_to(ctrl.x, ctrl.y, b.x, b.y);
    }
    pb.close();
    out.extend(pb.finish());
    // The bubbles, from just outside the bumps to the tail.
    let d = tail.sub(c);
    let norm = ((d.x / rx).powi(2) + (d.y / ry).powi(2)).sqrt();
    if norm > 1.0 && norm.is_finite() {
        let dir = d.scale(1.0 / d.length().max(0.001));
        let edge = c.add(d.scale(1.0 / norm));
        let big = (rx.min(ry) * 0.22).clamp(3.0, 22.0);
        let radii = [big, big * 0.68, big * 0.42];
        let [.., last] = radii;
        let from = edge.add(dir.scale(bulge + big * 1.2));
        let to = tail.sub(dir.scale(last));
        let room = to.sub(from);
        // Only where there is room for them, going towards the tail.
        if room.x * dir.x + room.y * dir.y > 0.0 {
            for (i, r) in radii.iter().enumerate() {
                let at = from.lerp(to, i as f32 / 2.0);
                out.extend(circle(at, *r));
            }
        }
    }
    out
}

/// A rounded box with a tail running from the side facing `tail` to `tail`, as one outline.
pub fn callout_path(rect: Rect, tail: Pt, radius: f32) -> Option<Path> {
    let rad = radius.max(0.0).min(rect.w / 2.0).min(rect.h / 2.0);
    if rect.inflate(-1.0).contains(tail) || rect.is_empty() {
        return rounded_rect(rect, rad);
    }
    let c = rect.center();
    let d = tail.sub(c);
    let side = facing_side(rect, tail);
    // Where on that side the line from the centre to the tail crosses, kept clear of the corners.
    let base_half = (if side.is_multiple_of(2) { rect.w } else { rect.h } * 0.14).clamp(6.0, 36.0);
    let (lo_x, hi_x) = (rect.x + rad + base_half, rect.right() - rad - base_half);
    let (lo_y, hi_y) = (rect.y + rad + base_half, rect.bottom() - rad - base_half);
    let cross = |along: f32, lo: f32, hi: f32| if lo <= hi { along.clamp(lo, hi) } else { (lo + hi) / 2.0 };
    let at_x = |y: f32| if d.y.abs() > f32::EPSILON { c.x + d.x * (y - c.y) / d.y } else { c.x };
    let at_y = |x: f32| if d.x.abs() > f32::EPSILON { c.y + d.y * (x - c.x) / d.x } else { c.y };
    let k = 0.552_284_8 * rad;
    let (x0, y0, x1, y1) = (rect.x, rect.y, rect.right(), rect.bottom());
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + rad, y0);
    if side == 0 {
        let m = cross(at_x(y0), lo_x, hi_x);
        pb.line_to(m - base_half, y0);
        pb.line_to(tail.x, tail.y);
        pb.line_to(m + base_half, y0);
    }
    pb.line_to(x1 - rad, y0);
    pb.cubic_to(x1 - rad + k, y0, x1, y0 + rad - k, x1, y0 + rad);
    if side == 1 {
        let m = cross(at_y(x1), lo_y, hi_y);
        pb.line_to(x1, m - base_half);
        pb.line_to(tail.x, tail.y);
        pb.line_to(x1, m + base_half);
    }
    pb.line_to(x1, y1 - rad);
    pb.cubic_to(x1, y1 - rad + k, x1 - rad + k, y1, x1 - rad, y1);
    if side == 2 {
        let m = cross(at_x(y1), lo_x, hi_x);
        pb.line_to(m + base_half, y1);
        pb.line_to(tail.x, tail.y);
        pb.line_to(m - base_half, y1);
    }
    pb.line_to(x0 + rad, y1);
    pb.cubic_to(x0 + rad - k, y1, x0, y1 - rad + k, x0, y1 - rad);
    if side == 3 {
        let m = cross(at_y(x0), lo_y, hi_y);
        pb.line_to(x0, m + base_half);
        pb.line_to(tail.x, tail.y);
        pb.line_to(x0, m - base_half);
    }
    pb.line_to(x0, y0 + rad);
    pb.cubic_to(x0, y0 + rad - k, x0 + rad - k, y0, x0 + rad, y0);
    pb.close();
    pb.finish()
}

fn draw_spotlight(canvas: &mut Pixmap, rect: Rect, ellipse: bool, style: &Style) {
    let (w, h) = (canvas.width() as f32, canvas.height() as f32);
    let mut pb = PathBuilder::new();
    pb.push_rect(match tiny_skia::Rect::from_xywh(0.0, 0.0, w, h) {
        Some(r) => r,
        None => return,
    });
    let hole = if ellipse { oval(rect) } else { rounded_rect(rect, 6.0) };
    let Some(hole) = hole else { return };
    pb.push_path(&hole);
    let Some(path) = pb.finish() else { return };
    let mut dim = Paint::default();
    dim.set_color(tiny_skia::Color::from_rgba(0.0, 0.0, 0.0, style.strength.clamp(0.0, 1.0)).unwrap_or(tiny_skia::Color::BLACK));
    dim.anti_alias = true;
    canvas.fill_path(&path, &dim, FillRule::EvenOdd, Transform::identity(), None);
    if style.width > 0.0 && visible(style.stroke) {
        canvas.stroke_path(&hole, &paint(style.stroke), &stroke_of(style.width, style.dashed, LineCap::Round), Transform::identity(), None);
    }
}

fn draw_magnify(canvas: &mut Pixmap, rect: Rect, style: &Style) {
    let r = (rect.w.min(rect.h) / 2.0).max(1.0);
    let c = rect.center();
    let zoom = style.strength.clamp(1.0, 8.0);
    // What is under the lens, enlarged about its middle.
    let snapshot = canvas.clone();
    let lens = Rect::new(c.x - r, c.y - r, 2.0 * r, 2.0 * r);
    let plain = Style { opacity: 1.0, ..*style };
    layered(canvas, lens.inflate(style.width), &plain, BlendMode::SourceOver, style.opacity, &|layer, t| {
        let Some(disc) = circle(c, r) else { return };
        // The pattern is placed in canvas coordinates; filling with `t` moves both into the layer.
        let place = Transform::from_translate(c.x, c.y).pre_scale(zoom, zoom).pre_translate(-c.x, -c.y);
        let pattern = Pattern::new(snapshot.as_ref(), SpreadMode::Pad, FilterQuality::Bicubic, 1.0, place);
        let p = Paint { shader: pattern, anti_alias: true, ..Paint::default() };
        layer.fill_path(&disc, &p, FillRule::Winding, t, None);
        if style.width > 0.0 && visible(style.stroke) {
            layer.stroke_path(&disc, &paint(style.stroke), &stroke_of(style.width, false, LineCap::Round), t, None);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::{Kind, StampKind};

    fn doc() -> Document {
        Document::new(Raster::filled(200, 120, Color::WHITE).unwrap())
    }

    fn px(r: &Raster, x: u32, y: u32) -> Color {
        r.pixel(x, y).unwrap()
    }

    #[test]
    fn every_kind_draws_without_panicking() {
        let mut d = doc();
        let key = d.add_image(Raster::filled(10, 10, Color::BLACK).unwrap()).unwrap();
        let r = Rect::new(20.0, 20.0, 80.0, 50.0);
        let shapes = vec![
            Shape::Arrow { from: Pt::new(5.0, 5.0), to: Pt::new(150.0, 100.0), heads: ArrowHeads::Both, bend: 0.0, head: Default::default() },
            Shape::Arrow { from: Pt::new(5.0, 5.0), to: Pt::new(5.2, 5.0), heads: ArrowHeads::End, bend: 0.0, head: Default::default() },
            Shape::Line { from: Pt::new(0.0, 0.0), to: Pt::new(199.0, 119.0) },
            Shape::Rect { rect: r, radius: 12.0 },
            Shape::Ellipse { rect: r },
            Shape::Pen { points: vec![Pt::new(1.0, 1.0), Pt::new(30.0, 40.0), Pt::new(60.0, 10.0)] },
            Shape::Pen { points: vec![Pt::new(1.0, 1.0)] },
            Shape::highlight(Rect::new(10.0, 50.0, 180.0, 20.0)),
            Shape::highlight(Rect::new(-40.0, 100.0, 400.0, 0.0)),
            Shape::Text { rect: r, text: "Hello, world".into() },
            Shape::Callout { rect: r, tail: Pt::new(180.0, 110.0), text: "Look here".into(), form: CalloutForm::Rounded },
            Shape::Callout { rect: r, tail: Pt::new(5.0, 110.0), text: "Square".into(), form: CalloutForm::Square },
            Shape::Callout { rect: r, tail: Pt::new(180.0, 45.0), text: "Oval".into(), form: CalloutForm::Oval },
            Shape::Callout { rect: r, tail: Pt::new(150.0, 115.0), text: "Cloud".into(), form: CalloutForm::Cloud },
            Shape::Callout { rect: r, tail: Pt::new(140.0, 45.0), text: "Arrow".into(), form: CalloutForm::Arrow },
            Shape::Callout { rect: Rect::new(10.0, 10.0, 0.5, 0.5), tail: Pt::new(f32::NAN, 1.0), text: String::new(), form: CalloutForm::Cloud },
            Shape::step(Pt::new(150.0, 30.0), 7),
            Shape::Blur { rect: r },
            Shape::Pixelate { rect: Rect::new(-50.0, -50.0, 500.0, 500.0) },
            Shape::Spotlight { rect: r, ellipse: true },
            Shape::Magnify { rect: Rect::new(120.0, 50.0, 60.0, 60.0) },
            Shape::Image { rect: r, image: key },
        ];
        for s in shapes {
            let kind = s.kind();
            d.add(s, kind.default_style()).unwrap();
        }
        for k in StampKind::ALL {
            d.add(Shape::Stamp { rect: Rect::new(150.0, 70.0, 40.0, 40.0), stamp: k }, Kind::Stamp.default_style()).unwrap();
        }
        for (i, badge) in screengrabcraft_model::BadgeShape::ALL.into_iter().enumerate() {
            for labels in screengrabcraft_model::StepLabels::ALL {
                let center = Pt::new(20.0 + i as f32 * 30.0, 100.0);
                d.add(Shape::Step { center, number: 3888, labels, badge }, Kind::Step.default_style()).unwrap();
            }
        }
        let out = render(&d).unwrap();
        assert_eq!((out.width(), out.height()), (200, 120));
    }

    /// A filled triangle and star colour their inside and leave their box's corners alone; a
    /// curved arrow is drawn along its bow, with its heads at its ends, and every polygon and
    /// bend draws.
    #[test]
    fn polygons_fill_their_outline_and_arrows_bow() {
        let style = Style { fill: Color::BLACK, stroke: Color::BLACK, width: 2.0, shadow: false, ..Style::default() };
        let mut d = doc();
        d.add(Shape::Polygon { rect: Rect::new(10.0, 10.0, 80.0, 70.0), sides: 3, star: false }, style).unwrap();
        d.add(Shape::Polygon { rect: Rect::new(110.0, 10.0, 80.0, 80.0), sides: 5, star: true }, style).unwrap();
        let out = render(&d).unwrap();
        assert_eq!(px(&out, 50, 60), Color::BLACK, "inside the triangle");
        assert_eq!(px(&out, 14, 14), Color::WHITE, "a corner of its box");
        assert_eq!(px(&out, 150, 52), Color::BLACK, "the star's middle");
        assert_eq!(px(&out, 114, 86), Color::WHITE, "between its points");
        let mut d = doc();
        let ink = Style { stroke: Color::BLACK, width: 4.0, shadow: false, ..Style::default() };
        d.add(
            Shape::Arrow { from: Pt::new(20.0, 100.0), to: Pt::new(180.0, 100.0), heads: ArrowHeads::Both, bend: 0.3, head: Default::default() },
            ink,
        )
        .unwrap();
        let out = render(&d).unwrap();
        let top = screengrabcraft_model::object::bend_middle(Pt::new(20.0, 100.0), Pt::new(180.0, 100.0), 0.3);
        assert_eq!(px(&out, top.x as u32, top.y as u32), Color::BLACK, "along the bow at {top:?}");
        assert_eq!(px(&out, 100, 98), Color::WHITE, "not where a straight one would be");
        for (x, y) in [(21, 99), (179, 99)] {
            assert!(px(&out, x, y).r < 160, "a head at each end, at {x},{y}");
        }
        // Every head shape is drawn at the tip; an open head leaves the inside of its V empty,
        // a filled one doesn't.
        for shape in HeadShape::ALL {
            let mut d = doc();
            d.add(Shape::Arrow { from: Pt::new(20.0, 60.0), to: Pt::new(180.0, 60.0), heads: ArrowHeads::End, bend: 0.0, head: shape }, ink).unwrap();
            let out = render(&d).unwrap();
            let (hl, hh) = arrow_head(4.0);
            let near_tip = (0..6).any(|dx| px(&out, 179 - dx, 60).r < 100);
            assert!(near_tip, "{shape:?}: drawn at its tip");
            // Inside the head, between its edge and the shaft (behind the notch, for the notched one).
            let notched = px(&out, (180.0 - hl * 0.85) as u32, (60.0 - hh * 0.55) as u32);
            let inside = px(&out, (180.0 - hl * 0.9) as u32, (60.0 - hh * 0.38) as u32);
            match shape {
                HeadShape::Notched => assert!(notched.r < 128, "{shape:?} is filled there: {notched:?}"),
                HeadShape::Triangle => assert!(inside.r < 128, "{shape:?} is filled there: {inside:?}"),
                HeadShape::Open => assert!(inside.r > 128, "{shape:?} is open there: {inside:?}"),
                _ => {}
            }
        }
        let mut d = doc();
        for sides in 0..14 {
            for star in [false, true] {
                d.add(
                    Shape::Polygon { rect: Rect::new(f32::from(sides as u8) * 12.0, 5.0, 10.0 + f32::from(sides as u8), 30.0), sides, star },
                    style,
                )
                .unwrap();
            }
        }
        for bend in [-2.0, -0.5, 0.0, 0.01, 1.5, f32::NAN] {
            d.add(Shape::Arrow { from: Pt::new(10.0, 60.0), to: Pt::new(11.0, 61.0), heads: ArrowHeads::Both, bend, head: Default::default() }, ink)
                .unwrap();
            d.add(
                Shape::Arrow { from: Pt::new(10.0, 60.0), to: Pt::new(190.0, 110.0), heads: ArrowHeads::Start, bend, head: Default::default() },
                ink,
            )
            .unwrap();
        }
        assert!(render(&d).is_some());
    }

    /// Each badge is drawn in its own outline, its label inside it: a diamond leaves the corners
    /// of its box alone, a square fills them, a pin reaches down to its point; and a long label
    /// (Roman numerals) is made to fit.
    #[test]
    fn step_badges_take_their_shape_and_fit_their_labels() {
        use screengrabcraft_model::{BadgeShape, StepLabels};
        let style = Style { fill: Color::BLACK, stroke: Color::TRANSPARENT, text_color: Color::WHITE, shadow: false, ..Kind::Step.default_style() };
        let r = step_radius(style.font_size);
        let c = Pt::new(60.0, 50.0);
        let draw = |badge: BadgeShape, labels: StepLabels, number: u32| {
            let mut d = doc();
            d.add(Shape::Step { center: c, number, labels, badge }, style).unwrap();
            render(&d).unwrap()
        };
        let corner = |out: &Raster| px(out, (c.x + r * 0.8) as u32, (c.y - r * 0.8) as u32);
        assert_eq!(corner(&draw(BadgeShape::Square, StepLabels::Numbers, 1)), Color::BLACK);
        assert_eq!(corner(&draw(BadgeShape::Diamond, StepLabels::Numbers, 1)), Color::WHITE);
        assert_eq!(corner(&draw(BadgeShape::Circle, StepLabels::Numbers, 1)), Color::WHITE);
        let pin = draw(BadgeShape::Pin, StepLabels::Numbers, 1);
        assert_eq!(px(&pin, c.x as u32, (c.y + r * 1.6) as u32), Color::BLACK, "the pin's point");
        assert_eq!(px(&draw(BadgeShape::Circle, StepLabels::Numbers, 1), c.x as u32, (c.y + r * 1.6) as u32), Color::WHITE);
        // Every label shows, in the badge's middle, and stays clear of its sides, however long.
        for badge in BadgeShape::ALL {
            for (labels, n) in [(StepLabels::Roman, 38), (StepLabels::Upper, 702), (StepLabels::Numbers, 9999), (StepLabels::RomanLower, 8)] {
                let out = draw(badge, labels, n);
                let side = badge.reach(r).0;
                let mut letters = 0;
                for y in (c.y - r * 0.4) as u32..(c.y + r * 0.4) as u32 {
                    for x in (c.x - side * 0.5) as u32..(c.x + side * 0.5) as u32 {
                        if px(&out, x, y).r > 200 {
                            letters += 1;
                        }
                    }
                }
                assert!(letters > 4, "{badge:?} {labels:?}: the label shows ({letters})");
                for y in (c.y - r * 0.3) as u32..(c.y + r * 0.3) as u32 {
                    // How far the badge reaches to each side on this row.
                    let dy = (y as f32 + 0.5 - c.y).abs();
                    let half = match badge {
                        BadgeShape::Diamond => side * (1.0 - dy / badge.reach(r).1),
                        BadgeShape::Circle | BadgeShape::Pin => (r * r - dy * dy).sqrt(),
                        _ => side,
                    };
                    for x in [(c.x - half + 3.0) as u32, (c.x + half - 3.0) as u32] {
                        assert_eq!(px(&out, x, y), Color::BLACK, "{badge:?} {labels:?}: the label is clear of the side at {x},{y}");
                    }
                }
                // Outside its box nothing is drawn.
                let b = Shape::Step { center: c, number: n, labels, badge }.bounds(&style);
                for (x, y) in [(b.x as u32 - 2, c.y as u32), (b.right() as u32 + 2, c.y as u32)] {
                    assert_eq!(px(&out, x, y), Color::WHITE, "{badge:?} {labels:?} at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn a_red_box_is_red_and_the_rest_untouched() {
        let mut d = doc();
        let style = Style { stroke: Color::RED, width: 4.0, shadow: false, ..Style::default() };
        d.add(Shape::Rect { rect: Rect::new(20.0, 20.0, 100.0, 60.0), radius: 0.0 }, style).unwrap();
        let out = render(&d).unwrap();
        assert_eq!(px(&out, 20, 50), Color::RED, "on the border");
        assert_eq!(px(&out, 70, 50), Color::WHITE, "inside an unfilled box");
        assert_eq!(px(&out, 5, 5), Color::WHITE);
    }

    #[test]
    fn shadows_and_opacity() {
        let mut d = doc();
        let style = Style { fill: Color::BLACK, stroke: Color::TRANSPARENT, shadow: true, width: 4.0, ..Style::default() };
        d.add(Shape::Rect { rect: Rect::new(20.0, 20.0, 40.0, 40.0), radius: 0.0 }, style).unwrap();
        let half = Style { fill: Color::BLACK, stroke: Color::TRANSPARENT, opacity: 0.5, ..Style::default() };
        d.add(Shape::Rect { rect: Rect::new(120.0, 20.0, 40.0, 40.0), radius: 0.0 }, half).unwrap();
        let out = render(&d).unwrap();
        let shadow = px(&out, 63, 63);
        assert!(shadow.r < 255 && shadow.r > 100, "a soft shadow below and right: {shadow:?}");
        let faded = px(&out, 140, 40);
        assert!((120..=135).contains(&faded.r), "half see-through: {faded:?}");
    }

    #[test]
    fn effects_change_only_their_area() {
        let mut base = Raster::filled(100, 40, Color::WHITE).unwrap();
        for x in 0..100 {
            for y in 0..40 {
                if (x / 4 + y / 4) % 2 == 0 {
                    let i = ((y * 100 + x) * 4) as usize;
                    base.pixels_mut()[i..i + 3].copy_from_slice(&[0, 0, 0]);
                }
            }
        }
        let mut d = Document::new(base.clone());
        d.add(Shape::Blur { rect: Rect::new(0.0, 0.0, 50.0, 40.0) }, Kind::Blur.default_style()).unwrap();
        let out = render(&d).unwrap();
        let blurred = px(&out, 25, 20);
        assert!(blurred.r > 40 && blurred.r < 215, "{blurred:?}");
        assert_eq!(px(&out, 80, 20), base.pixel(80, 20).unwrap(), "outside the blur");
        let mut d = Document::new(Raster::filled(100, 40, Color::WHITE).unwrap());
        d.add(Shape::Spotlight { rect: Rect::new(40.0, 10.0, 20.0, 20.0), ellipse: false }, Kind::Spotlight.default_style()).unwrap();
        let out = render(&d).unwrap();
        assert!(px(&out, 5, 5).r < 120, "darkened outside");
        assert_eq!(px(&out, 50, 20), Color::WHITE, "lit inside");
    }

    #[test]
    fn hidden_objects_are_skipped_and_the_base_is_cached() {
        let mut d = doc();
        let id =
            d.add(Shape::Rect { rect: Rect::new(0.0, 0.0, 200.0, 120.0), radius: 0.0 }, Style { fill: Color::BLACK, ..Style::default() }).unwrap();
        let mut r = Renderer::new();
        let shown = r.render(&d, &Options::default()).unwrap();
        let hidden = r.render(&d, &Options { hide: vec![id], no_text: None }).unwrap();
        assert_ne!(shown.data(), hidden.data());
        assert_eq!(&hidden.data()[..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn callout_tails_leave_the_facing_side() {
        let r = Rect::new(100.0, 100.0, 100.0, 50.0);
        for tail in [Pt::new(150.0, 20.0), Pt::new(300.0, 125.0), Pt::new(150.0, 260.0), Pt::new(0.0, 125.0), Pt::new(0.0, 0.0)] {
            let p = callout_path(r, tail, 10.0).unwrap();
            let b = p.bounds();
            assert!(b.left() <= tail.x + 0.5 && b.right() >= tail.x - 0.5 && b.top() <= tail.y + 0.5 && b.bottom() >= tail.y - 0.5, "{tail:?} {b:?}");
        }
        let inside = callout_path(r, Pt::new(150.0, 125.0), 10.0).unwrap();
        assert!((inside.bounds().width() - 100.0).abs() < 0.5);
    }

    #[test]
    fn every_callout_form_reaches_its_tail_and_holds_its_box() {
        let r = Rect::new(100.0, 100.0, 120.0, 60.0);
        for form in CalloutForm::ALL {
            for spot in screengrabcraft_model::TailSpot::ALL {
                let tail = spot.place(form, r);
                let paths = callout_paths(r, tail, form, 10.0);
                assert!(!paths.is_empty(), "{form:?} {spot:?}");
                let b = paths.iter().map(|p| p.bounds()).fold(None, |acc: Option<(f32, f32, f32, f32)>, b| {
                    Some(match acc {
                        None => (b.left(), b.top(), b.right(), b.bottom()),
                        Some((l, t, r, bt)) => (l.min(b.left()), t.min(b.top()), r.max(b.right()), bt.max(b.bottom())),
                    })
                });
                let (left, top, right, bottom) = b.unwrap();
                let reach = 0.5 + if form == CalloutForm::Cloud { 3.0 } else { 0.0 };
                assert!(
                    left <= tail.x + reach && right >= tail.x - reach && top <= tail.y + reach && bottom >= tail.y - reach,
                    "{form:?} {spot:?}: {tail:?} {b:?}"
                );
                assert!(left <= r.x + 0.5 && right >= r.right() - 0.5 && top <= r.y + 0.5 && bottom >= r.bottom() - 0.5);
            }
        }
        // An arrow callout pointing right is a box with a point on its right: five corners.
        let arrow = arrow_callout_path(r, Pt::new(260.0, 130.0)).unwrap();
        assert_eq!(arrow.points().len(), 5);
        assert!((arrow.bounds().right() - 260.0).abs() < 0.01 && (arrow.bounds().left() - 100.0).abs() < 0.01);
        // A cloud's tail is three bubbles after the cloud; with the tail inside, none.
        assert_eq!(cloud_paths(r, Pt::new(160.0, 260.0), 6.0).len(), 4);
        assert_eq!(cloud_paths(r, r.center(), 6.0).len(), 1);
    }

    #[test]
    fn text_outlines_go_round_the_letters() {
        let count = |outline: Color| {
            let mut d = Document::new(Raster::filled(200, 120, Color::BLACK).unwrap());
            let style = Style {
                text_color: Color::RED,
                text_outline: outline,
                text_outline_width: 4.0,
                font_size: 60.0,
                stroke: Color::TRANSPARENT,
                shadow: false,
                ..Style::default()
            };
            d.add(Shape::Text { rect: Rect::new(10.0, 10.0, 180.0, 100.0), text: "HI".into() }, style).unwrap();
            let out = render(&d).unwrap();
            let px = |c: Color| out.pixels().as_chunks::<4>().0.iter().filter(|p| p[..3] == [c.r, c.g, c.b]).count();
            (px(Color::WHITE), px(Color::RED))
        };
        let (white, red) = count(Color::WHITE);
        assert!(white > 200 && red > 200, "a white outline round red letters: {white} {red}");
        let (white, red_plain) = count(Color::TRANSPARENT);
        assert_eq!(white, 0, "no outline without a colour");
        assert!(red_plain >= red, "the outline is outside the letters, not over them");
        // Out of range from a file: kept to its bounds.
        let s = Style { text_outline_width: f32::INFINITY, ..Style::default() }.clamped();
        assert_eq!(s.text_outline_width, Style::default().text_outline_width);
        let s = Style { text_outline_width: 900.0, ..Style::default() }.clamped();
        assert_eq!(s.text_outline_width, screengrabcraft_model::object::MAX_TEXT_OUTLINE);
    }

    #[test]
    fn a_filled_arrow_callout_is_coloured_at_its_point() {
        let mut d = doc();
        let style = Style { fill: Color::rgb(0, 0, 255), stroke: Color::TRANSPARENT, width: 0.0, shadow: false, ..Style::default() };
        let rect = Rect::new(20.0, 30.0, 100.0, 60.0);
        d.add(Shape::Callout { rect, tail: Pt::new(170.0, 60.0), text: String::new(), form: CalloutForm::Arrow }, style).unwrap();
        d.add(
            Shape::Callout { rect: Rect::new(150.0, 5.0, 40.0, 20.0), tail: Pt::new(170.0, 15.0), text: String::new(), form: CalloutForm::Oval },
            style,
        )
        .unwrap();
        let out = render(&d).unwrap();
        assert_eq!(px(&out, 140, 60), Color::rgb(0, 0, 255), "in the point");
        assert_eq!(px(&out, 140, 35), Color::WHITE, "beside the point");
        assert_eq!(px(&out, 151, 6), Color::WHITE, "an oval leaves its box's corners alone");
    }
}

#[cfg(test)]
mod stroke_tests {
    use super::*;
    use screengrabcraft_model::Kind;

    fn dist_to_polyline(p: (f32, f32), pts: &[Pt]) -> f32 {
        let mut best = f32::INFINITY;
        for w in pts.windows(2) {
            if let [a, b] = w {
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let len2 = dx * dx + dy * dy;
                let t = if len2 > 0.0 { (((p.0 - a.x) * dx + (p.1 - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                let (cx, cy) = (a.x + t * dx, a.y + t * dy);
                best = best.min(((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt());
            }
        }
        best
    }

    /// The owner's scribble with a wide highlighter (when it drew strokes, as the pen does) left a
    /// wedge (a "crease") where it turned sharply: nothing of a stroke is drawn far from where it
    /// went.
    #[test]
    fn a_sharply_turning_stroke_has_no_crease() {
        let scribbles: Vec<Vec<Pt>> = vec![
            vec![Pt::new(30.0, 40.0), Pt::new(160.0, 44.0), Pt::new(164.0, 48.0), Pt::new(32.0, 54.0), Pt::new(30.0, 58.0), Pt::new(160.0, 66.0)],
            vec![Pt::new(40.0, 100.0), Pt::new(100.0, 20.0), Pt::new(102.0, 22.0), Pt::new(104.0, 100.0), Pt::new(106.0, 98.0), Pt::new(170.0, 20.0)],
            vec![Pt::new(50.0, 60.0), Pt::new(150.0, 60.0), Pt::new(150.5, 60.5), Pt::new(50.0, 61.0)],
            // A tight curl drawn fast: short steps round a small circle, then away.
            (0..40)
                .map(|i| {
                    let a = i as f32 * 0.35;
                    Pt::new(100.0 + 8.0 * a.cos(), 60.0 + 8.0 * a.sin())
                })
                .chain([Pt::new(180.0, 100.0)])
                .collect(),
            vec![Pt::new(20.0, 100.0), Pt::new(90.0, 30.0), Pt::new(93.0, 29.0), Pt::new(95.0, 31.0), Pt::new(96.0, 36.0), Pt::new(97.0, 100.0)],
        ];
        for (kind, width) in [(Kind::Pen, 24.0), (Kind::Pen, 50.0)] {
            for points in &scribbles {
                let mut d = Document::new(Raster::filled(200, 120, Color::WHITE).unwrap());
                let shape = Shape::Pen { points: points.clone() };
                let style = Style { width, shadow: false, ..kind.default_style() };
                d.add(shape, style).unwrap();
                let out = render(&d).unwrap();
                let (first, last) = (points.first().copied().unwrap(), points.last().copied().unwrap());
                let mut stray = Vec::new();
                for y in 0..120u32 {
                    for x in 0..200u32 {
                        let p = (x as f32 + 0.5, y as f32 + 0.5);
                        let near_end = [first, last].iter().any(|e| ((p.0 - e.x).powi(2) + (p.1 - e.y).powi(2)).sqrt() < width);
                        if near_end || dist_to_polyline(p, points) <= width / 2.0 + 6.0 {
                            continue;
                        }
                        if out.pixel(x, y) != Some(Color::WHITE) {
                            stray.push((x, y));
                        }
                    }
                }
                assert!(stray.is_empty(), "{kind:?} {points:?}: {} pixels drawn away from the stroke, e.g. {:?}", stray.len(), stray.first());
            }
        }
    }

    /// A highlight tints its box as a marker does, and nothing outside it: white turns the
    /// marker's colour, black stays black (it's multiplied in), the edges are straight.
    #[test]
    fn a_highlight_tints_its_box_only() {
        let mut pic = Raster::filled(200, 120, Color::WHITE).unwrap();
        // A black line across, at y = 60.
        for px in pic.pixels_mut()[60 * 200 * 4..61 * 200 * 4].as_chunks_mut::<4>().0 {
            *px = [0, 0, 0, 255];
        }
        let mut d = Document::new(pic);
        d.add(Shape::highlight(Rect::new(40.0, 40.0, 100.0, 40.0)), Kind::Highlight.default_style()).unwrap();
        let out = render(&d).unwrap();
        let inside = out.pixel(90, 50).unwrap();
        assert!(inside.r > 230 && inside.g > 190 && inside.b < 160, "yellow inside: {inside:?}");
        assert_eq!(out.pixel(90, 60), Some(Color::BLACK), "dark stays dark");
        assert_eq!(out.pixel(41, 79), Some(inside), "to its corners");
        for (x, y) in [(38, 50), (142, 50), (90, 38), (90, 82), (10, 10)] {
            assert_eq!(out.pixel(x, y), Some(Color::WHITE), "nothing outside at {x},{y}");
        }
    }

    #[test]
    fn a_turned_annotation_is_drawn_turned_round_its_middle() {
        let mut d = Document::new(Raster::filled(200, 120, Color::WHITE).unwrap());
        let style = Style { fill: Color::RED, stroke: Color::TRANSPARENT, width: 0.0, shadow: false, ..Style::default() };
        let id = d.add(Shape::Rect { rect: Rect::new(60.0, 55.0, 80.0, 10.0), radius: 0.0 }, style).unwrap();
        let red = |out: &Raster, x, y| out.pixel(x, y) == Some(Color::RED);
        let flat = render(&d).unwrap();
        assert!(red(&flat, 65, 60) && !red(&flat, 100, 25));
        d.get_mut(id).unwrap().rotation = 90.0;
        let out = render(&d).unwrap();
        assert!(red(&out, 100, 25) && red(&out, 100, 95), "tall and narrow now");
        assert!(!red(&out, 65, 60) && !red(&out, 135, 60));
        // A highlight turned still multiplies into the picture; a blur doesn't turn.
        let hl = d.add(Shape::highlight(Rect::new(10.0, 10.0, 40.0, 10.0)), Kind::Highlight.default_style()).unwrap();
        d.get_mut(hl).unwrap().rotation = 45.0;
        let out = render(&d).unwrap();
        assert!(out.pixel(30, 15).is_some_and(|c| c.b < 200 && c.r > 200), "{:?}", out.pixel(30, 15));
        let blur = d.add(Shape::Blur { rect: Rect::new(150.0, 80.0, 30.0, 30.0) }, Style::default()).unwrap();
        d.get_mut(blur).unwrap().rotation = 30.0;
        assert_eq!(d.get(blur).unwrap().turn(), 0.0);
        // Far off the picture, nothing to draw.
        d.get_mut(id).unwrap().shape.translate(Pt::new(1.0e6, 1.0e6));
        assert!(render(&d).is_some());
    }
}
