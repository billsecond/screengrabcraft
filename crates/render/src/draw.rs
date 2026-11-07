//! Drawing a document: the picture, then each annotation in order. Annotations that change the
//! pixels under them (blur, pixelate, spotlight, magnify) work on what has been drawn so far, so
//! a blur hides an arrow drawn before it and not one drawn after.

use screengrabcraft_model::object::{arrow_head, step_radius};
use screengrabcraft_model::{ArrowHeads, CalloutForm, Color, Document, Object, ObjectId, Pt, Raster, Rect, Shape, Style};
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

/// Draws one annotation onto the canvas.
pub fn draw_object(canvas: &mut Pixmap, doc: &Document, o: &Object) {
    let style = o.style.clamped();
    let bounds = o.shape.bounds(&style);
    match &o.shape {
        Shape::Arrow { from, to, heads } => {
            let (from, to, heads) = (*from, *to, *heads);
            with_style(canvas, bounds, &style, &|c, t| draw_arrow(c, t, from, to, heads, &style));
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
        Shape::Pen { points } => {
            with_style(canvas, bounds, &style, &|c, t| {
                if let Some(path) = smooth(points) {
                    c.stroke_path(&path, &paint(style.stroke), &stroke_of(style.width, style.dashed, LineCap::Round), t, None);
                }
            });
        }
        Shape::Highlight { points } => {
            // Drawn opaque in a layer, then multiplied in at the colour's own transparency, so a
            // stroke that crosses itself isn't darker where it does.
            let alpha = f32::from(style.stroke.a) / 255.0;
            let opaque = style.stroke.with_alpha(255);
            let plain = Style { shadow: false, ..style };
            layered(canvas, bounds, &plain, BlendMode::Multiply, alpha, &|c, t| {
                if let Some(path) = smooth(points) {
                    let s = Stroke { width: style.width.max(1.0), line_cap: LineCap::Square, line_join: LineJoin::Round, ..Stroke::default() };
                    c.stroke_path(&path, &paint(opaque), &s, t, None);
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
        Shape::Step { center, number } => {
            let (center, number) = (*center, *number);
            with_style(canvas, bounds, &style, &|c, t| draw_step(c, t, center, number, &style));
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

fn draw_arrow(c: &mut Pixmap, t: Transform, from: Pt, to: Pt, heads: ArrowHeads, style: &Style) {
    let d = to.sub(from);
    let len = d.length();
    let ink = paint(style.stroke);
    if len < 0.5 {
        if let Some(dot) = circle(from, style.width / 2.0) {
            c.fill_path(&dot, &ink, FillRule::Winding, t, None);
        }
        return;
    }
    let dir = d.scale(1.0 / len);
    let normal = Pt::new(-dir.y, dir.x);
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
    let a = if start_head { from.add(dir.scale(head_len * 0.7)) } else { from };
    let b = if end_head { to.sub(dir.scale(head_len * 0.7)) } else { to };
    let mut pb = PathBuilder::new();
    pb.move_to(a.x, a.y);
    pb.line_to(b.x, b.y);
    if let Some(shaft) = pb.finish() {
        c.stroke_path(
            &shaft,
            &ink,
            &stroke_of(style.width, style.dashed, if heads == ArrowHeads::None { LineCap::Round } else { LineCap::Butt }),
            t,
            None,
        );
    }
    let head = |tip: Pt, dir: Pt| -> Option<Path> {
        let base = tip.sub(dir.scale(head_len));
        let notch = tip.sub(dir.scale(head_len * 0.78));
        let left = base.add(normal.scale(head_half));
        let right = base.sub(normal.scale(head_half));
        let mut pb = PathBuilder::new();
        pb.move_to(tip.x, tip.y);
        pb.line_to(left.x, left.y);
        pb.line_to(notch.x, notch.y);
        pb.line_to(right.x, right.y);
        pb.close();
        pb.finish()
    };
    if end_head && let Some(p) = head(to, dir) {
        c.fill_path(&p, &ink, FillRule::Winding, t, None);
    }
    if start_head && let Some(p) = head(from, dir.scale(-1.0)) {
        c.fill_path(&p, &ink, FillRule::Winding, t, None);
    }
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

fn draw_step(c: &mut Pixmap, t: Transform, center: Pt, number: u32, style: &Style) {
    let r = step_radius(style.font_size);
    let Some(disc) = circle(center, r) else { return };
    c.fill_path(&disc, &paint(if visible(style.fill) { style.fill } else { Color::RED }), FillRule::Winding, t, None);
    if style.width > 0.0 && visible(style.stroke) {
        c.stroke_path(&disc, &paint(style.stroke), &stroke_of(style.width, false, LineCap::Round), t, None);
    }
    let label = number.to_string();
    let size = if label.len() > 2 { style.font_size * 0.7 } else { style.font_size };
    let s = Style { font_size: size, bold: true, mono: false, italic: false, ..*style };
    if let Some(path) = centered_label(&label, &s, center) {
        c.fill_path(&path, &paint(style.text_color), FillRule::Winding, t, None);
    }
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
            Shape::Arrow { from: Pt::new(5.0, 5.0), to: Pt::new(150.0, 100.0), heads: ArrowHeads::Both },
            Shape::Arrow { from: Pt::new(5.0, 5.0), to: Pt::new(5.2, 5.0), heads: ArrowHeads::End },
            Shape::Line { from: Pt::new(0.0, 0.0), to: Pt::new(199.0, 119.0) },
            Shape::Rect { rect: r, radius: 12.0 },
            Shape::Ellipse { rect: r },
            Shape::Pen { points: vec![Pt::new(1.0, 1.0), Pt::new(30.0, 40.0), Pt::new(60.0, 10.0)] },
            Shape::Pen { points: vec![Pt::new(1.0, 1.0)] },
            Shape::Highlight { points: vec![Pt::new(10.0, 60.0), Pt::new(190.0, 60.0)] },
            Shape::Text { rect: r, text: "Hello, world".into() },
            Shape::Callout { rect: r, tail: Pt::new(180.0, 110.0), text: "Look here".into(), form: CalloutForm::Rounded },
            Shape::Callout { rect: r, tail: Pt::new(5.0, 110.0), text: "Square".into(), form: CalloutForm::Square },
            Shape::Callout { rect: r, tail: Pt::new(180.0, 45.0), text: "Oval".into(), form: CalloutForm::Oval },
            Shape::Callout { rect: r, tail: Pt::new(150.0, 115.0), text: "Cloud".into(), form: CalloutForm::Cloud },
            Shape::Callout { rect: r, tail: Pt::new(140.0, 45.0), text: "Arrow".into(), form: CalloutForm::Arrow },
            Shape::Callout { rect: Rect::new(10.0, 10.0, 0.5, 0.5), tail: Pt::new(f32::NAN, 1.0), text: String::new(), form: CalloutForm::Cloud },
            Shape::Step { center: Pt::new(150.0, 30.0), number: 7 },
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
        let out = render(&d).unwrap();
        assert_eq!((out.width(), out.height()), (200, 120));
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
