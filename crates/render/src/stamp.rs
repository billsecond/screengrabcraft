//! Stamps: small pictograms (a check mark, a cross, a star…) drawn in code, filling their box.

use screengrabcraft_model::{Color, Pt, Rect, StampKind, Style};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::draw::{centered_label, skia_color};

fn paint(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(skia_color(c, 1.0));
    p.anti_alias = true;
    p
}

/// The colour a stamp is filled with when it is first placed.
pub fn default_fill(kind: StampKind) -> Color {
    kind.color()
}

/// Draws `kind` filling the square inside `rect`.
pub fn draw(c: &mut Pixmap, t: Transform, rect: Rect, kind: StampKind, style: &Style) {
    let size = rect.w.min(rect.h).max(2.0);
    let center = rect.center();
    let r = size / 2.0;
    let fill = paint(if style.fill.a > 0 { style.fill } else { default_fill(kind) });
    let ink = if style.stroke.a > 0 { style.stroke } else { Color::WHITE };
    let border = |c: &mut Pixmap, path: &tiny_skia::Path| {
        if style.width > 0.0 && style.stroke.a > 0 {
            let s = Stroke { width: style.width.min(size * 0.12), line_join: LineJoin::Round, ..Stroke::default() };
            c.stroke_path(path, &paint(style.stroke), &s, t, None);
        }
    };
    let mark = Stroke { width: size * 0.12, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    let at = |x: f32, y: f32| Pt::new(center.x + x * r, center.y + y * r);
    match kind {
        StampKind::Check | StampKind::Cross | StampKind::Question | StampKind::Info => {
            let Some(disc) = PathBuilder::from_circle(center.x, center.y, r * 0.96) else { return };
            c.fill_path(&disc, &fill, FillRule::Winding, t, None);
            border(c, &disc);
            let mut pb = PathBuilder::new();
            match kind {
                StampKind::Check => {
                    let (a, b, d) = (at(-0.45, 0.02), at(-0.12, 0.36), at(0.5, -0.34));
                    pb.move_to(a.x, a.y);
                    pb.line_to(b.x, b.y);
                    pb.line_to(d.x, d.y);
                }
                StampKind::Cross => {
                    let (a, b, d, e) = (at(-0.38, -0.38), at(0.38, 0.38), at(0.38, -0.38), at(-0.38, 0.38));
                    pb.move_to(a.x, a.y);
                    pb.line_to(b.x, b.y);
                    pb.move_to(d.x, d.y);
                    pb.line_to(e.x, e.y);
                }
                _ => {
                    let label = if kind == StampKind::Question { "?" } else { "i" };
                    let s = Style { font_size: size * 0.62, bold: true, mono: false, italic: false, ..*style };
                    if let Some(glyph) = centered_label(label, &s, center) {
                        c.fill_path(&glyph, &paint(ink), FillRule::Winding, t, None);
                    }
                    return;
                }
            }
            if let Some(path) = pb.finish() {
                c.stroke_path(&path, &paint(ink), &mark, t, None);
            }
        }
        StampKind::Star => {
            let mut pb = PathBuilder::new();
            for i in 0..10 {
                let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                let rr = if i % 2 == 0 { 0.98 } else { 0.42 };
                let p = at(angle.cos() * rr, angle.sin() * rr + 0.06);
                if i == 0 {
                    pb.move_to(p.x, p.y);
                } else {
                    pb.line_to(p.x, p.y);
                }
            }
            pb.close();
            if let Some(star) = pb.finish() {
                c.fill_path(&star, &fill, FillRule::Winding, t, None);
                border(c, &star);
            }
        }
        StampKind::Warning => {
            let mut pb = PathBuilder::new();
            let (top, left, right) = (at(0.0, -0.88), at(-0.96, 0.8), at(0.96, 0.8));
            pb.move_to(top.x, top.y);
            pb.line_to(right.x, right.y);
            pb.line_to(left.x, left.y);
            pb.close();
            if let Some(tri) = pb.finish() {
                let rounder = Stroke { width: size * 0.1, line_join: LineJoin::Round, ..Stroke::default() };
                c.fill_path(&tri, &fill, FillRule::Winding, t, None);
                c.stroke_path(&tri, &fill, &rounder, t, None);
                border(c, &tri);
            }
            let s = Style { font_size: size * 0.55, bold: true, mono: false, italic: false, ..*style };
            if let Some(glyph) = centered_label("!", &s, at(0.0, 0.18)) {
                c.fill_path(&glyph, &paint(ink), FillRule::Winding, t, None);
            }
        }
        StampKind::Heart => {
            let mut pb = PathBuilder::new();
            let (b, top) = (at(0.0, 0.9), at(0.0, -0.45));
            pb.move_to(b.x, b.y);
            let (c1, c2, l) = (at(-0.6, 0.45), at(-1.05, 0.05), at(-0.95, -0.35));
            pb.cubic_to(c1.x, c1.y, c2.x, c2.y, l.x, l.y);
            let (c1, c2) = (at(-0.85, -0.95), at(-0.15, -0.95));
            pb.cubic_to(c1.x, c1.y, c2.x, c2.y, top.x, top.y);
            let (c1, c2, rr) = (at(0.15, -0.95), at(0.85, -0.95), at(0.95, -0.35));
            pb.cubic_to(c1.x, c1.y, c2.x, c2.y, rr.x, rr.y);
            let (c1, c2) = (at(1.05, 0.05), at(0.6, 0.45));
            pb.cubic_to(c1.x, c1.y, c2.x, c2.y, b.x, b.y);
            pb.close();
            if let Some(heart) = pb.finish() {
                c.fill_path(&heart, &fill, FillRule::Winding, t, None);
                border(c, &heart);
            }
        }
        StampKind::Plus | StampKind::Minus => {
            let Some(disc) = PathBuilder::from_circle(center.x, center.y, r * 0.96) else { return };
            c.fill_path(&disc, &fill, FillRule::Winding, t, None);
            border(c, &disc);
            let mut pb = PathBuilder::new();
            let (a, b) = (at(-0.45, 0.0), at(0.45, 0.0));
            pb.move_to(a.x, a.y);
            pb.line_to(b.x, b.y);
            if kind == StampKind::Plus {
                let (a, b) = (at(0.0, -0.45), at(0.0, 0.45));
                pb.move_to(a.x, a.y);
                pb.line_to(b.x, b.y);
            }
            if let Some(path) = pb.finish() {
                c.stroke_path(&path, &paint(ink), &mark, t, None);
            }
        }
        StampKind::ArrowUp | StampKind::ArrowDown | StampKind::ArrowLeft | StampKind::ArrowRight => {
            let Some(disc) = PathBuilder::from_circle(center.x, center.y, r * 0.96) else { return };
            c.fill_path(&disc, &fill, FillRule::Winding, t, None);
            border(c, &disc);
            // An arrow pointing right, turned to point the stamp's way.
            let (dx, dy) = match kind {
                StampKind::ArrowUp => (0.0, -1.0),
                StampKind::ArrowDown => (0.0, 1.0),
                StampKind::ArrowLeft => (-1.0, 0.0),
                _ => (1.0, 0.0),
            };
            let turn = |x: f32, y: f32| at(x * dx - y * dy, x * dy + y * dx);
            let mut pb = PathBuilder::new();
            let (tail, tip) = (turn(-0.48, 0.0), turn(0.46, 0.0));
            pb.move_to(tail.x, tail.y);
            pb.line_to(tip.x, tip.y);
            let (l, rr) = (turn(0.08, -0.38), turn(0.08, 0.38));
            pb.move_to(l.x, l.y);
            pb.line_to(tip.x, tip.y);
            pb.line_to(rr.x, rr.y);
            if let Some(path) = pb.finish() {
                c.stroke_path(&path, &paint(ink), &mark, t, None);
            }
        }
        StampKind::NoEntry => {
            // An octagon with a bar across it.
            let corners: Vec<Pt> = (0..8)
                .map(|i| {
                    let a = std::f32::consts::PI / 8.0 + i as f32 * std::f32::consts::FRAC_PI_4;
                    at(a.cos() * 0.98, a.sin() * 0.98)
                })
                .collect();
            if let Some(oct) = closed(&corners) {
                c.fill_path(&oct, &fill, FillRule::Winding, t, None);
                border(c, &oct);
            }
            if let Some(bar) = crate::draw::rounded_rect(Rect::new(center.x - r * 0.6, center.y - r * 0.15, r * 1.2, r * 0.3), r * 0.06) {
                c.fill_path(&bar, &paint(ink), FillRule::Winding, t, None);
            }
        }
        StampKind::Idea => {
            // A bulb over a screw base.
            let Some(bulb) = PathBuilder::from_circle(center.x, center.y - r * 0.22, r * 0.62) else { return };
            let neck = closed(&[at(-0.34, 0.22), at(0.34, 0.22), at(0.26, 0.56), at(-0.26, 0.56)]);
            for p in std::iter::once(bulb).chain(neck) {
                c.fill_path(&p, &fill, FillRule::Winding, t, None);
                border(c, &p);
            }
            let base = Stroke { width: size * 0.07, line_cap: LineCap::Round, ..Stroke::default() };
            let mut pb = PathBuilder::new();
            for (y, w) in [(0.68, 0.24), (0.84, 0.16)] {
                let (a, b) = (at(-w, y), at(w, y));
                pb.move_to(a.x, a.y);
                pb.line_to(b.x, b.y);
            }
            if let Some(p) = pb.finish() {
                c.stroke_path(&p, &paint(Color::rgb(0x5D, 0x5D, 0x5D)), &base, t, None);
            }
            // A shine on the glass.
            let shine = Stroke { width: size * 0.06, line_cap: LineCap::Round, ..Stroke::default() };
            let mut pb = PathBuilder::new();
            let (a, ctl, b) = (at(-0.36, -0.2), at(-0.36, -0.52), at(-0.08, -0.6));
            pb.move_to(a.x, a.y);
            pb.quad_to(ctl.x, ctl.y, b.x, b.y);
            if let Some(p) = pb.finish() {
                c.stroke_path(&p, &paint(Color::WHITE.with_alpha(200)), &shine, t, None);
            }
        }
        StampKind::Pin => {
            // A round head with a point under it, a hole in its middle.
            let tip = at(0.0, 0.98);
            let head = at(0.0, -0.3);
            let hr = r * 0.62;
            let d = tip.y - head.y;
            let half = (hr / d).clamp(-1.0, 1.0).acos();
            let down = std::f32::consts::FRAC_PI_2;
            let mut pts = vec![tip];
            for i in 0..=32 {
                let a = down + half + (std::f32::consts::TAU - 2.0 * half) * i as f32 / 32.0;
                pts.push(Pt::new(head.x + hr * a.cos(), head.y + hr * a.sin()));
            }
            if let Some(pin) = closed(&pts) {
                c.fill_path(&pin, &fill, FillRule::Winding, t, None);
                border(c, &pin);
            }
            if let Some(hole) = PathBuilder::from_circle(head.x, head.y, hr * 0.42) {
                c.fill_path(&hole, &paint(ink), FillRule::Winding, t, None);
            }
        }
        StampKind::Flag => {
            let pole = Stroke { width: size * 0.08, line_cap: LineCap::Round, ..Stroke::default() };
            let mut pb = PathBuilder::new();
            let (a, b) = (at(-0.62, -0.88), at(-0.62, 0.92));
            pb.move_to(a.x, a.y);
            pb.line_to(b.x, b.y);
            if let Some(p) = pb.finish() {
                c.stroke_path(&p, &paint(Color::rgb(0x45, 0x45, 0x45)), &pole, t, None);
            }
            // The cloth, waving.
            let mut pb = PathBuilder::new();
            let p0 = at(-0.58, -0.82);
            pb.move_to(p0.x, p0.y);
            let (c1, c2, p1) = (at(-0.2, -1.0), at(0.3, -0.62), at(0.82, -0.8));
            pb.cubic_to(c1.x, c1.y, c2.x, c2.y, p1.x, p1.y);
            let p2 = at(0.82, 0.1);
            pb.line_to(p2.x, p2.y);
            let (c1, c2, p3) = (at(0.3, 0.28), at(-0.2, -0.1), at(-0.58, 0.08));
            pb.cubic_to(c1.x, c1.y, c2.x, c2.y, p3.x, p3.y);
            pb.close();
            if let Some(cloth) = pb.finish() {
                c.fill_path(&cloth, &fill, FillRule::Winding, t, None);
                border(c, &cloth);
            }
        }
        StampKind::Lock => {
            // The shackle, then the body over it, a keyhole in it.
            let shackle = Stroke { width: size * 0.12, line_cap: LineCap::Butt, ..Stroke::default() };
            let mut pb = PathBuilder::new();
            let (a, b) = (at(-0.4, 0.0), at(0.4, 0.0));
            pb.move_to(a.x, a.y);
            let top = at(0.0, -0.95);
            let (c1, c2) = (at(-0.4, -0.95), at(0.4, -0.95));
            let (m1, m2) = (at(-0.4, -0.45), at(0.4, -0.45));
            pb.line_to(m1.x, m1.y);
            pb.cubic_to(c1.x, c1.y, top.x - r * 0.2, top.y, top.x, top.y);
            pb.cubic_to(top.x + r * 0.2, top.y, c2.x, c2.y, m2.x, m2.y);
            pb.line_to(b.x, b.y);
            if let Some(p) = pb.finish() {
                c.stroke_path(&p, &paint(Color::rgb(0x78, 0x78, 0x78)), &shackle, t, None);
            }
            if let Some(body) = crate::draw::rounded_rect(Rect::new(center.x - r * 0.72, center.y - r * 0.12, r * 1.44, r * 1.06), r * 0.16) {
                c.fill_path(&body, &fill, FillRule::Winding, t, None);
                border(c, &body);
            }
            let hole = PathBuilder::from_circle(center.x, center.y + r * 0.3, r * 0.14);
            let slot = closed(&[at(-0.07, 0.34), at(0.07, 0.34), at(0.1, 0.66), at(-0.1, 0.66)]);
            for p in hole.into_iter().chain(slot) {
                c.fill_path(&p, &paint(ink), FillRule::Winding, t, None);
            }
        }
        StampKind::Clock => {
            let Some(disc) = PathBuilder::from_circle(center.x, center.y, r * 0.96) else { return };
            c.fill_path(&disc, &fill, FillRule::Winding, t, None);
            border(c, &disc);
            if let Some(face) = PathBuilder::from_circle(center.x, center.y, r * 0.78) {
                c.fill_path(&face, &paint(Color::WHITE), FillRule::Winding, t, None);
            }
            let hands = Stroke { width: size * 0.08, line_cap: LineCap::Round, ..Stroke::default() };
            let mut pb = PathBuilder::new();
            let (a, m, b) = (at(0.0, -0.58), at(0.0, 0.0), at(0.4, 0.18));
            pb.move_to(a.x, a.y);
            pb.line_to(m.x, m.y);
            pb.line_to(b.x, b.y);
            if let Some(p) = pb.finish() {
                c.stroke_path(&p, &fill, &hands, t, None);
            }
        }
        StampKind::Search => {
            let lens = Stroke { width: size * 0.14, line_cap: LineCap::Round, ..Stroke::default() };
            let g = at(-0.16, -0.16);
            if let Some(glass) = PathBuilder::from_circle(g.x, g.y, r * 0.55) {
                c.fill_path(&glass, &paint(Color::WHITE.with_alpha(230)), FillRule::Winding, t, None);
                c.stroke_path(&glass, &fill, &lens, t, None);
            }
            let handle = Stroke { width: size * 0.2, line_cap: LineCap::Round, ..Stroke::default() };
            let mut pb = PathBuilder::new();
            let (a, b) = (at(0.3, 0.3), at(0.78, 0.78));
            pb.move_to(a.x, a.y);
            pb.line_to(b.x, b.y);
            if let Some(p) = pb.finish() {
                c.stroke_path(&p, &fill, &handle, t, None);
            }
        }
        StampKind::Bookmark => {
            if let Some(mark) = closed(&[at(-0.55, -0.92), at(0.55, -0.92), at(0.55, 0.92), at(0.0, 0.48), at(-0.55, 0.92)]) {
                c.fill_path(&mark, &fill, FillRule::Winding, t, None);
                border(c, &mark);
            }
        }
        StampKind::Gear => {
            // Eight teeth round a wheel with a hole.
            let teeth = 8;
            let mut pts = Vec::with_capacity(teeth * 4);
            for i in 0..teeth {
                let base = i as f32 * std::f32::consts::TAU / teeth as f32;
                let w = std::f32::consts::TAU / teeth as f32;
                for (da, rr) in [(-0.3, 0.72), (-0.17, 0.98), (0.17, 0.98), (0.3, 0.72)] {
                    let a = base + da * w;
                    pts.push(at(a.cos() * rr, a.sin() * rr));
                }
            }
            let mut pb = PathBuilder::new();
            if let Some((first, rest)) = pts.split_first() {
                pb.move_to(first.x, first.y);
                for p in rest {
                    pb.line_to(p.x, p.y);
                }
                pb.close();
            }
            pb.push_circle(center.x, center.y, r * 0.3);
            if let Some(gear) = pb.finish() {
                c.fill_path(&gear, &fill, FillRule::EvenOdd, t, None);
                border(c, &gear);
            }
        }
        StampKind::Home => {
            if let Some(house) =
                closed(&[at(0.0, -0.92), at(0.95, -0.02), at(0.72, -0.02), at(0.72, 0.9), at(-0.72, 0.9), at(-0.72, -0.02), at(-0.95, -0.02)])
            {
                c.fill_path(&house, &fill, FillRule::Winding, t, None);
                border(c, &house);
            }
            if let Some(door) = crate::draw::rounded_rect(Rect::new(center.x - r * 0.2, center.y + r * 0.34, r * 0.4, r * 0.56), r * 0.05) {
                c.fill_path(&door, &paint(ink), FillRule::Winding, t, None);
            }
        }
        StampKind::Mail => {
            let Some(env) = crate::draw::rounded_rect(Rect::new(center.x - r * 0.94, center.y - r * 0.62, r * 1.88, r * 1.24), r * 0.1) else {
                return;
            };
            c.fill_path(&env, &fill, FillRule::Winding, t, None);
            border(c, &env);
            let flap = Stroke { width: size * 0.07, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
            let mut pb = PathBuilder::new();
            let (a, m, b) = (at(-0.8, -0.48), at(0.0, 0.12), at(0.8, -0.48));
            pb.move_to(a.x, a.y);
            pb.line_to(m.x, m.y);
            pb.line_to(b.x, b.y);
            if let Some(p) = pb.finish() {
                c.stroke_path(&p, &paint(ink), &flap, t, None);
            }
        }
        StampKind::Pencil => {
            // Slanted, its point at the lower left: along its length from the point (u, 0 to 1)
            // and across it (v).
            let (tip, end) = (at(-0.82, 0.82), at(0.82, -0.82));
            let axis = end.sub(tip);
            let across = Pt::new(-axis.y, axis.x).scale(1.0 / axis.length().max(0.001));
            let along = |u: f32, v: f32| tip.add(axis.scale(u)).add(across.scale(v * r));
            let w = 0.26;
            if let Some(body) = closed(&[along(0.3, -w), along(1.0, -w), along(1.0, w), along(0.3, w)]) {
                c.fill_path(&body, &fill, FillRule::Winding, t, None);
                border(c, &body);
            }
            if let Some(wood) = closed(&[along(0.0, 0.0), along(0.3, -w), along(0.3, w)]) {
                c.fill_path(&wood, &paint(Color::rgb(0xF1, 0xC2, 0x8A)), FillRule::Winding, t, None);
            }
            if let Some(lead) = closed(&[along(0.0, 0.0), along(0.11, -w * 0.37), along(0.11, w * 0.37)]) {
                c.fill_path(&lead, &paint(Color::rgb(0x33, 0x33, 0x33)), FillRule::Winding, t, None);
            }
        }
        StampKind::Happy
        | StampKind::Laughing
        | StampKind::Wink
        | StampKind::Neutral
        | StampKind::Sad
        | StampKind::Surprised
        | StampKind::Angry
        | StampKind::Love
        | StampKind::Cool => draw_face(c, t, center, r, kind, &fill, &border),
    }
}

/// A face: a round head in the stamp's colour, and its eyes, brows and mouth in a dark ink
/// (drawn here, in the simplest lines: nothing of anyone's emoji).
fn draw_face(c: &mut Pixmap, t: Transform, center: Pt, r: f32, kind: StampKind, fill: &Paint<'_>, border: &dyn Fn(&mut Pixmap, &tiny_skia::Path)) {
    let at = |x: f32, y: f32| Pt::new(center.x + x * r, center.y + y * r);
    let Some(head) = PathBuilder::from_circle(center.x, center.y, r * 0.96) else { return };
    c.fill_path(&head, fill, FillRule::Winding, t, None);
    border(c, &head);
    let ink = paint(FACE_INK);
    let line = Stroke { width: r * 0.13, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    let stroke = |c: &mut Pixmap, pts: &[(f32, f32)], curve: bool| {
        let mut pb = PathBuilder::new();
        let p: Vec<Pt> = pts.iter().map(|(x, y)| at(*x, *y)).collect();
        match (p.as_slice(), curve) {
            ([a, ctl, b], true) => {
                pb.move_to(a.x, a.y);
                pb.quad_to(ctl.x, ctl.y, b.x, b.y);
            }
            (all, _) => {
                if let Some((first, rest)) = all.split_first() {
                    pb.move_to(first.x, first.y);
                    for q in rest {
                        pb.line_to(q.x, q.y);
                    }
                }
            }
        }
        if let Some(path) = pb.finish() {
            c.stroke_path(&path, &ink, &line, t, None);
        }
    };
    let dot_eye = |c: &mut Pixmap, x: f32, big: f32| {
        if let Some(e) = oval(at(x, -0.22), r * 0.1 * big, r * 0.15 * big) {
            c.fill_path(&e, &ink, FillRule::Winding, t, None);
        }
    };
    let smile = |c: &mut Pixmap| stroke(c, &[(-0.46, 0.2), (0.0, 0.72), (0.46, 0.2)], true);
    match kind {
        StampKind::Happy => {
            dot_eye(c, -0.32, 1.0);
            dot_eye(c, 0.32, 1.0);
            smile(c);
        }
        StampKind::Laughing => {
            // Eyes shut with laughing, a wide open mouth.
            stroke(c, &[(-0.5, -0.14), (-0.32, -0.42), (-0.14, -0.14)], true);
            stroke(c, &[(0.14, -0.14), (0.32, -0.42), (0.5, -0.14)], true);
            let mut pb = PathBuilder::new();
            let (a, ctl, b) = (at(-0.56, 0.12), at(0.0, 1.0), at(0.56, 0.12));
            pb.move_to(a.x, a.y);
            pb.quad_to(ctl.x, ctl.y, b.x, b.y);
            pb.close();
            if let Some(mouth) = pb.finish() {
                c.fill_path(&mouth, &ink, FillRule::Winding, t, None);
            }
        }
        StampKind::Wink => {
            dot_eye(c, -0.32, 1.0);
            stroke(c, &[(0.16, -0.2), (0.32, -0.3), (0.48, -0.2)], true);
            smile(c);
        }
        StampKind::Neutral => {
            dot_eye(c, -0.32, 1.0);
            dot_eye(c, 0.32, 1.0);
            stroke(c, &[(-0.34, 0.4), (0.34, 0.4)], false);
        }
        StampKind::Sad => {
            dot_eye(c, -0.32, 1.0);
            dot_eye(c, 0.32, 1.0);
            stroke(c, &[(-0.4, 0.56), (0.0, 0.14), (0.4, 0.56)], true);
        }
        StampKind::Surprised => {
            dot_eye(c, -0.32, 1.25);
            dot_eye(c, 0.32, 1.25);
            if let Some(o) = oval(at(0.0, 0.42), r * 0.17, r * 0.22) {
                c.fill_path(&o, &ink, FillRule::Winding, t, None);
            }
        }
        StampKind::Angry => {
            dot_eye(c, -0.32, 1.0);
            dot_eye(c, 0.32, 1.0);
            stroke(c, &[(-0.56, -0.52), (-0.16, -0.36)], false);
            stroke(c, &[(0.56, -0.52), (0.16, -0.36)], false);
            stroke(c, &[(-0.38, 0.52), (0.0, 0.24), (0.38, 0.52)], true);
        }
        StampKind::Love => {
            // Heart eyes.
            for x in [-0.32, 0.32] {
                let e = at(x, -0.2);
                let s = r * 0.22;
                let mut pb = PathBuilder::new();
                pb.move_to(e.x, e.y + s);
                pb.cubic_to(e.x - s * 1.4, e.y + s * 0.1, e.x - s * 0.9, e.y - s * 1.2, e.x, e.y - s * 0.4);
                pb.cubic_to(e.x + s * 0.9, e.y - s * 1.2, e.x + s * 1.4, e.y + s * 0.1, e.x, e.y + s);
                pb.close();
                if let Some(h) = pb.finish() {
                    c.fill_path(&h, &paint(Color::rgb(0xE5, 0x39, 0x35)), FillRule::Winding, t, None);
                }
            }
            smile(c);
        }
        StampKind::Cool => {
            // Dark glasses and a smile.
            let mut pb = PathBuilder::new();
            for x in [-0.32f32, 0.32] {
                let l = at(x - 0.26, -0.34);
                let rr = at(x + 0.26, -0.34);
                let low = at(x, 0.02);
                pb.move_to(l.x, l.y);
                pb.line_to(rr.x, rr.y);
                pb.quad_to(rr.x, low.y, low.x, low.y);
                pb.quad_to(l.x, low.y, l.x, l.y);
                pb.close();
            }
            if let Some(g) = pb.finish() {
                c.fill_path(&g, &ink, FillRule::Winding, t, None);
            }
            stroke(c, &[(-0.7, -0.3), (0.7, -0.3)], false);
            stroke(c, &[(-0.36, 0.34), (0.0, 0.66), (0.36, 0.34)], true);
        }
        _ => {}
    }
}

/// The dark lines of a face (eyes, mouth), whatever its colour.
const FACE_INK: Color = Color::rgb(0x4A, 0x2E, 0x0C);

/// A path through `points`, closed.
fn closed(points: &[Pt]) -> Option<tiny_skia::Path> {
    let (first, rest) = points.split_first()?;
    let mut pb = PathBuilder::new();
    pb.move_to(first.x, first.y);
    for p in rest {
        pb.line_to(p.x, p.y);
    }
    pb.close();
    pb.finish()
}

fn oval(center: Pt, rx: f32, ry: f32) -> Option<tiny_skia::Path> {
    PathBuilder::from_oval(tiny_skia::Rect::from_xywh(center.x - rx, center.y - ry, (2.0 * rx).max(0.1), (2.0 * ry).max(0.1))?)
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::{Document, ImageFormat, Kind, Raster, Shape};

    use super::*;

    /// Every stamp draws something in its box and nothing outside it (its border aside); a
    /// sheet of them all is saved to look at when `SGC_SHOTS` names a folder.
    #[test]
    fn every_stamp_draws_inside_its_box() {
        let cols = 9u32;
        let rows = (StampKind::ALL.len() as u32).div_ceil(cols);
        let mut sheet = Document::new(Raster::filled(cols * 128, rows * 128, Color::WHITE).unwrap());
        for (i, k) in StampKind::ALL.into_iter().enumerate() {
            let style = Style { fill: k.color(), shadow: false, ..Kind::Stamp.default_style() };
            let mut d = Document::new(Raster::filled(80, 80, Color::WHITE).unwrap());
            let rect = Rect::new(16.0, 16.0, 48.0, 48.0);
            d.add(Shape::Stamp { rect, stamp: k }, style).unwrap();
            let out = crate::render(&d).unwrap();
            let reach = rect.inflate(style.width / 2.0 + 1.0);
            let (mut inside, mut outside) = (0, 0);
            for y in 0..80 {
                for x in 0..80 {
                    if out.pixel(x, y) != Some(Color::WHITE) {
                        if reach.contains(Pt::new(x as f32 + 0.5, y as f32 + 0.5)) {
                            inside += 1;
                        } else {
                            outside += 1;
                        }
                    }
                }
            }
            assert!(inside > 300, "{k:?} draws: {inside}");
            assert_eq!(outside, 0, "{k:?} stays in its box");
            let (cx, cy) = ((i as u32 % cols) as f32 * 128.0, (i as u32 / cols) as f32 * 128.0);
            sheet.add(Shape::Stamp { rect: Rect::new(cx + 12.0, cy + 12.0, 104.0, 104.0), stamp: k }, style).unwrap();
        }
        // Each looks different from the others.
        let drawn: Vec<Vec<u8>> = StampKind::ALL
            .into_iter()
            .map(|k| {
                let mut d = Document::new(Raster::filled(40, 40, Color::WHITE).unwrap());
                d.add(
                    Shape::Stamp { rect: Rect::new(4.0, 4.0, 32.0, 32.0), stamp: k },
                    Style { fill: Color::BLACK, shadow: false, ..Kind::Stamp.default_style() },
                )
                .unwrap();
                crate::render(&d).unwrap().pixels().to_vec()
            })
            .collect();
        for (i, a) in drawn.iter().enumerate() {
            for (j, b) in drawn.iter().enumerate().skip(i + 1) {
                assert_ne!(a, b, "{:?} and {:?} look the same", StampKind::ALL.get(i), StampKind::ALL.get(j));
            }
        }
        if let Ok(dir) = std::env::var("SGC_SHOTS") {
            let png = crate::codec::encode(&crate::render(&sheet).unwrap(), ImageFormat::Png, 100).unwrap();
            std::fs::write(format!("{dir}/stamps.png"), png).unwrap();
        }
    }
}
