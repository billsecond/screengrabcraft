//! Annotations: what is drawn on a picture and stays editable (moved, resized, restyled,
//! deleted) until the picture is flattened or exported.
//!
//! Each annotation is an [`Object`]: an id, a [`Shape`] (where it is and what it is) and a
//! [`Style`] (how it looks). Geometry lives here; drawing is `screengrabcraft-render`'s.

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::geom::{Pt, Rect, distance_to_polyline, distance_to_segment};

/// An annotation's id, unique in its document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectId(pub u64);

/// Which ends of an arrow carry a head.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArrowHeads {
    #[default]
    End,
    Start,
    Both,
    None,
}

/// Stamps: small pictograms drawn in code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StampKind {
    #[default]
    Check,
    Cross,
    Star,
    Question,
    Warning,
    Info,
    Heart,
}

impl StampKind {
    pub const ALL: [StampKind; 7] =
        [StampKind::Check, StampKind::Cross, StampKind::Star, StampKind::Question, StampKind::Warning, StampKind::Info, StampKind::Heart];

    pub fn label(self) -> &'static str {
        match self {
            StampKind::Check => "Check mark",
            StampKind::Cross => "Cross",
            StampKind::Star => "Star",
            StampKind::Question => "Question",
            StampKind::Warning => "Warning",
            StampKind::Info => "Information",
            StampKind::Heart => "Heart",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// What an annotation is, and where.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Shape {
    Arrow {
        from: Pt,
        to: Pt,
        #[serde(default)]
        heads: ArrowHeads,
    },
    Line {
        from: Pt,
        to: Pt,
    },
    Rect {
        rect: Rect,
        #[serde(default)]
        radius: f32,
    },
    Ellipse {
        rect: Rect,
    },
    /// A freehand line.
    Pen {
        points: Vec<Pt>,
    },
    /// A see-through marker stroke, multiplied into the picture under it.
    Highlight {
        points: Vec<Pt>,
    },
    /// Text in a box (the box shows when the style has a fill or a border).
    Text {
        rect: Rect,
        text: String,
    },
    /// A speech balloon: a rounded box with a tail pointing at something, and text in it.
    Callout {
        rect: Rect,
        tail: Pt,
        text: String,
    },
    /// A numbered badge, for steps in an explanation.
    Step {
        center: Pt,
        number: u32,
    },
    /// Hides what is under it by blurring it.
    Blur {
        rect: Rect,
    },
    /// Hides what is under it in large squares.
    Pixelate {
        rect: Rect,
    },
    /// Darkens everything but this area.
    Spotlight {
        rect: Rect,
        #[serde(default)]
        ellipse: bool,
    },
    /// A round lens showing the picture under its middle enlarged.
    Magnify {
        rect: Rect,
    },
    Stamp {
        rect: Rect,
        stamp: StampKind,
    },
    /// A picture placed on the picture (pasted or dropped in); the pixels are the document's
    /// image with this key.
    Image {
        rect: Rect,
        image: u64,
    },
}

/// The kinds of shape, without their data: tools and per-kind styles are keyed by these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Arrow,
    Line,
    Rect,
    Ellipse,
    Pen,
    Highlight,
    Text,
    Callout,
    Step,
    Blur,
    Pixelate,
    Spotlight,
    Magnify,
    Stamp,
    Image,
}

impl Kind {
    pub const ALL: [Kind; 15] = [
        Kind::Arrow,
        Kind::Line,
        Kind::Rect,
        Kind::Ellipse,
        Kind::Pen,
        Kind::Highlight,
        Kind::Text,
        Kind::Callout,
        Kind::Step,
        Kind::Blur,
        Kind::Pixelate,
        Kind::Spotlight,
        Kind::Magnify,
        Kind::Stamp,
        Kind::Image,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Arrow => "Arrow",
            Kind::Line => "Line",
            Kind::Rect => "Rectangle",
            Kind::Ellipse => "Ellipse",
            Kind::Pen => "Pen",
            Kind::Highlight => "Highlighter",
            Kind::Text => "Text",
            Kind::Callout => "Callout",
            Kind::Step => "Step",
            Kind::Blur => "Blur",
            Kind::Pixelate => "Pixelate",
            Kind::Spotlight => "Spotlight",
            Kind::Magnify => "Magnify",
            Kind::Stamp => "Stamp",
            Kind::Image => "Picture",
        }
    }

    /// Whether this kind changes the pixels under it (rather than drawing over them).
    pub fn is_effect(self) -> bool {
        matches!(self, Kind::Blur | Kind::Pixelate | Kind::Spotlight | Kind::Magnify)
    }

    /// Whether the style's text settings mean anything for this kind.
    pub fn has_text(self) -> bool {
        matches!(self, Kind::Text | Kind::Callout | Kind::Step)
    }

    /// The look a new annotation of this kind starts with.
    pub fn default_style(self) -> Style {
        let base = Style::default();
        match self {
            Kind::Arrow => Style { width: 6.0, shadow: true, ..base },
            Kind::Line => Style { width: 4.0, ..base },
            Kind::Rect | Kind::Ellipse => Style { width: 4.0, shadow: true, ..base },
            Kind::Pen => Style { width: 4.0, ..base },
            Kind::Highlight => Style { stroke: Color::YELLOW.with_alpha(120), width: 22.0, ..base },
            Kind::Text => Style { stroke: Color::TRANSPARENT, text_color: Color::RED, font_size: 28.0, bold: true, shadow: false, ..base },
            Kind::Callout => Style {
                stroke: Color::RED,
                fill: Color::WHITE,
                text_color: Color::rgb(0x11, 0x11, 0x11),
                width: 3.0,
                font_size: 22.0,
                shadow: true,
                align: TextAlign::Center,
                ..base
            },
            Kind::Step => Style {
                fill: Color::RED,
                text_color: Color::WHITE,
                stroke: Color::WHITE,
                width: 2.0,
                font_size: 22.0,
                bold: true,
                shadow: true,
                ..base
            },
            Kind::Blur => Style { strength: 14.0, ..base },
            Kind::Pixelate => Style { strength: 12.0, ..base },
            Kind::Spotlight => Style { strength: 0.6, stroke: Color::TRANSPARENT, ..base },
            Kind::Magnify => Style { strength: 2.0, stroke: Color::WHITE, width: 4.0, shadow: true, ..base },
            Kind::Stamp => Style { fill: Color::rgb(0x2E, 0x9E, 0x4F), stroke: Color::WHITE, width: 3.0, shadow: true, ..base },
            Kind::Image => Style { stroke: Color::TRANSPARENT, width: 0.0, shadow: true, ..base },
        }
    }
}

/// How an annotation looks. Every field has a default, so files written by later versions with
/// more to say still open.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Style {
    /// Lines and borders.
    pub stroke: Color,
    /// The inside of closed shapes (transparent for none).
    pub fill: Color,
    /// Line width in pixels.
    pub width: f32,
    /// The whole annotation's opacity, 0 to 1.
    pub opacity: f32,
    /// A soft drop shadow under it.
    pub shadow: bool,
    pub dashed: bool,
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
    pub text_color: Color,
    pub align: TextAlign,
    /// What the effect kinds do, by kind: blur radius, pixel size, spotlight darkness (0 to 1),
    /// magnification.
    pub strength: f32,
}

impl Default for Style {
    fn default() -> Style {
        Style {
            stroke: Color::RED,
            fill: Color::TRANSPARENT,
            width: 4.0,
            opacity: 1.0,
            shadow: false,
            dashed: false,
            font_size: 24.0,
            bold: false,
            italic: false,
            mono: false,
            text_color: Color::RED,
            align: TextAlign::Left,
            strength: 10.0,
        }
    }
}

/// Bounds on style values, applied to anything read from outside.
pub const MAX_WIDTH: f32 = 200.0;
pub const MIN_FONT: f32 = 6.0;
pub const MAX_FONT: f32 = 400.0;

impl Style {
    /// The same style with every number in its range (NaN becomes the default).
    pub fn clamped(self) -> Style {
        let d = Style::default();
        let fix = |v: f32, lo: f32, hi: f32, def: f32| if v.is_finite() { v.clamp(lo, hi) } else { def };
        Style {
            width: fix(self.width, 0.0, MAX_WIDTH, d.width),
            opacity: fix(self.opacity, 0.0, 1.0, 1.0),
            font_size: fix(self.font_size, MIN_FONT, MAX_FONT, d.font_size),
            strength: fix(self.strength, 0.0, 200.0, d.strength),
            ..self
        }
    }

    /// Lengths in the style (line width, font size) scaled by `f`, for a picture resized by it.
    pub fn scaled(self, f: f32) -> Style {
        if !f.is_finite() || f <= 0.0 {
            return self;
        }
        Style { width: self.width * f, font_size: self.font_size * f, ..self }.clamped()
    }
}

/// The size of an arrow's head for a line width: its length along the line and its half width.
pub fn arrow_head(width: f32) -> (f32, f32) {
    let w = width.max(1.0);
    (w * 3.0 + 8.0, w * 1.7 + 5.0)
}

/// The radius of a step badge with this font size.
pub fn step_radius(font_size: f32) -> f32 {
    (font_size.max(MIN_FONT) * 0.85).max(8.0)
}

/// A place to drag an annotation's geometry by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    Start,
    End,
    Tail,
}

impl Handle {
    pub const BOX: [Handle; 8] =
        [Handle::TopLeft, Handle::Top, Handle::TopRight, Handle::Right, Handle::BottomRight, Handle::Bottom, Handle::BottomLeft, Handle::Left];

    fn on(self, r: Rect) -> Pt {
        let c = r.center();
        match self {
            Handle::TopLeft => Pt::new(r.x, r.y),
            Handle::Top => Pt::new(c.x, r.y),
            Handle::TopRight => Pt::new(r.right(), r.y),
            Handle::Right => Pt::new(r.right(), c.y),
            Handle::BottomRight => Pt::new(r.right(), r.bottom()),
            Handle::Bottom => Pt::new(c.x, r.bottom()),
            Handle::BottomLeft => Pt::new(r.x, r.bottom()),
            Handle::Left => Pt::new(r.x, c.y),
            Handle::Start | Handle::End | Handle::Tail => c,
        }
    }

    /// `r` with this handle dragged to `p`. With `keep_aspect`, a corner keeps the proportions.
    pub fn drag_rect(self, r: Rect, p: Pt, keep_aspect: bool) -> Rect {
        let p = p.finite();
        let (mut x0, mut y0, mut x1, mut y1) = (r.x, r.y, r.right(), r.bottom());
        match self {
            Handle::TopLeft => (x0, y0) = (p.x, p.y),
            Handle::Top => y0 = p.y,
            Handle::TopRight => (x1, y0) = (p.x, p.y),
            Handle::Right => x1 = p.x,
            Handle::BottomRight => (x1, y1) = (p.x, p.y),
            Handle::Bottom => y1 = p.y,
            Handle::BottomLeft => (x0, y1) = (p.x, p.y),
            Handle::Left => x0 = p.x,
            Handle::Start | Handle::End | Handle::Tail => return r,
        }
        let corner = matches!(self, Handle::TopLeft | Handle::TopRight | Handle::BottomRight | Handle::BottomLeft);
        if keep_aspect && corner && r.w > 0.0 && r.h > 0.0 {
            let aspect = r.w / r.h;
            let (w, h) = ((x1 - x0).abs(), (y1 - y0).abs());
            let (w, h) = if w / aspect > h { (w, w / aspect) } else { (h * aspect, h) };
            // Grow away from the fixed (opposite) corner.
            match self {
                Handle::TopLeft => (x0, y0) = (x1 - w * (x1 - x0).signum(), y1 - h * (y1 - y0).signum()),
                Handle::TopRight => (x1, y0) = (x0 + w * (x1 - x0).signum(), y1 - h * (y1 - y0).signum()),
                Handle::BottomRight => (x1, y1) = (x0 + w * (x1 - x0).signum(), y0 + h * (y1 - y0).signum()),
                _ => (x0, y1) = (x1 - w * (x1 - x0).signum(), y0 + h * (y1 - y0).signum()),
            }
        }
        Rect::from_points(Pt::new(x0, y0), Pt::new(x1, y1))
    }
}

/// An annotation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: ObjectId,
    #[serde(flatten)]
    pub shape: Shape,
    #[serde(default)]
    pub style: Style,
}

impl Shape {
    pub fn kind(&self) -> Kind {
        match self {
            Shape::Arrow { .. } => Kind::Arrow,
            Shape::Line { .. } => Kind::Line,
            Shape::Rect { .. } => Kind::Rect,
            Shape::Ellipse { .. } => Kind::Ellipse,
            Shape::Pen { .. } => Kind::Pen,
            Shape::Highlight { .. } => Kind::Highlight,
            Shape::Text { .. } => Kind::Text,
            Shape::Callout { .. } => Kind::Callout,
            Shape::Step { .. } => Kind::Step,
            Shape::Blur { .. } => Kind::Blur,
            Shape::Pixelate { .. } => Kind::Pixelate,
            Shape::Spotlight { .. } => Kind::Spotlight,
            Shape::Magnify { .. } => Kind::Magnify,
            Shape::Stamp { .. } => Kind::Stamp,
            Shape::Image { .. } => Kind::Image,
        }
    }

    /// The box of a box-shaped annotation.
    pub fn rect(&self) -> Option<Rect> {
        match self {
            Shape::Rect { rect, .. }
            | Shape::Ellipse { rect }
            | Shape::Text { rect, .. }
            | Shape::Callout { rect, .. }
            | Shape::Blur { rect }
            | Shape::Pixelate { rect }
            | Shape::Spotlight { rect, .. }
            | Shape::Magnify { rect }
            | Shape::Stamp { rect, .. }
            | Shape::Image { rect, .. } => Some(*rect),
            _ => None,
        }
    }

    fn rect_mut(&mut self) -> Option<&mut Rect> {
        match self {
            Shape::Rect { rect, .. }
            | Shape::Ellipse { rect }
            | Shape::Text { rect, .. }
            | Shape::Callout { rect, .. }
            | Shape::Blur { rect }
            | Shape::Pixelate { rect }
            | Shape::Spotlight { rect, .. }
            | Shape::Magnify { rect }
            | Shape::Stamp { rect, .. }
            | Shape::Image { rect, .. } => Some(rect),
            _ => None,
        }
    }

    /// The text a text-carrying annotation holds.
    pub fn text(&self) -> Option<&str> {
        match self {
            Shape::Text { text, .. } | Shape::Callout { text, .. } => Some(text),
            _ => None,
        }
    }

    pub fn text_mut(&mut self) -> Option<&mut String> {
        match self {
            Shape::Text { text, .. } | Shape::Callout { text, .. } => Some(text),
            _ => None,
        }
    }

    /// The area it covers, its line width included.
    pub fn bounds(&self, style: &Style) -> Rect {
        let half = style.width.max(0.0) / 2.0;
        match self {
            Shape::Arrow { from, to, heads } => {
                let grow = if *heads == ArrowHeads::None { half } else { arrow_head(style.width).1.max(half) };
                Rect::from_points(*from, *to).inflate(grow)
            }
            Shape::Line { from, to } => Rect::from_points(*from, *to).inflate(half),
            Shape::Pen { points } | Shape::Highlight { points } => Rect::bounding(points).inflate(half),
            Shape::Step { center, .. } => {
                let r = step_radius(style.font_size) + half;
                Rect::new(center.x - r, center.y - r, 2.0 * r, 2.0 * r)
            }
            Shape::Callout { rect, tail, .. } => rect.union(Rect::new(tail.x, tail.y, 0.0, 0.0)).inflate(half),
            Shape::Spotlight { rect, .. } | Shape::Blur { rect } | Shape::Pixelate { rect } => *rect,
            _ => self.rect().unwrap_or_default().inflate(half),
        }
    }

    /// The area its own points cover, without its line width or arrow heads.
    pub fn extent(&self) -> Rect {
        match self {
            Shape::Arrow { from, to, .. } | Shape::Line { from, to } => Rect::from_points(*from, *to),
            Shape::Pen { points } | Shape::Highlight { points } => Rect::bounding(points),
            Shape::Step { center, .. } => Rect::new(center.x, center.y, 0.0, 0.0),
            Shape::Callout { rect, tail, .. } => rect.union(Rect::new(tail.x, tail.y, 0.0, 0.0)),
            _ => self.rect().unwrap_or_default(),
        }
    }

    /// Whether `p` touches it, within `tol` pixels.
    pub fn hit(&self, style: &Style, p: Pt, tol: f32) -> bool {
        let reach = style.width.max(0.0) / 2.0 + tol;
        match self {
            Shape::Arrow { from, to, .. } | Shape::Line { from, to } => distance_to_segment(p, *from, *to) <= reach.max(arrow_reach(self, style)),
            Shape::Pen { points } | Shape::Highlight { points } => distance_to_polyline(p, points) <= reach,
            Shape::Rect { rect, .. } => {
                if !style.fill.is_transparent() {
                    rect.inflate(reach).contains(p)
                } else {
                    rect.inflate(reach).contains(p) && !rect.inflate(-reach).contains(p)
                }
            }
            Shape::Ellipse { rect } => {
                let c = rect.center();
                let (rx, ry) = (rect.w / 2.0, rect.h / 2.0);
                if rx <= 0.0 || ry <= 0.0 {
                    return distance_to_segment(p, rect.min(), rect.max()) <= reach;
                }
                let (dx, dy) = ((p.x - c.x) / rx, (p.y - c.y) / ry);
                let d = (dx * dx + dy * dy).sqrt();
                if !style.fill.is_transparent() { d <= 1.0 + reach / rx.min(ry) } else { (d - 1.0).abs() * rx.min(ry) <= reach }
            }
            Shape::Step { center, .. } => p.distance(*center) <= step_radius(style.font_size) + reach,
            Shape::Callout { rect, tail, .. } => rect.inflate(reach).contains(p) || distance_to_segment(p, rect.center(), *tail) <= reach.max(6.0),
            _ => self.rect().is_some_and(|r| r.inflate(tol).contains(p)),
        }
    }

    /// Where it can be dragged by.
    pub fn handles(&self, style: &Style) -> Vec<(Handle, Pt)> {
        match self {
            Shape::Arrow { from, to, .. } | Shape::Line { from, to } => vec![(Handle::Start, *from), (Handle::End, *to)],
            Shape::Step { .. } => Vec::new(),
            Shape::Pen { .. } | Shape::Highlight { .. } => {
                let b = self.bounds(style);
                Handle::BOX.iter().map(|h| (*h, h.on(b))).collect()
            }
            Shape::Callout { rect, tail, .. } => {
                let mut all: Vec<_> = Handle::BOX.iter().map(|h| (*h, h.on(*rect))).collect();
                all.push((Handle::Tail, *tail));
                all
            }
            _ => match self.rect() {
                Some(r) => Handle::BOX.iter().map(|h| (*h, h.on(r))).collect(),
                None => Vec::new(),
            },
        }
    }

    /// Moves `handle` to `p`.
    pub fn drag_handle(&mut self, style: &Style, handle: Handle, p: Pt, keep_aspect: bool) {
        let p = p.finite();
        match (self, handle) {
            (Shape::Arrow { from, .. } | Shape::Line { from, .. }, Handle::Start) => *from = p,
            (Shape::Arrow { to, .. } | Shape::Line { to, .. }, Handle::End) => *to = p,
            (Shape::Callout { tail, .. }, Handle::Tail) => *tail = p,
            (shape @ (Shape::Pen { .. } | Shape::Highlight { .. }), h) => {
                let old = shape.bounds(style).inflate(-style.width.max(0.0) / 2.0);
                let new = h.drag_rect(old, p, keep_aspect);
                let (sx, sy) = (if old.w > 0.0 { new.w / old.w } else { 1.0 }, if old.h > 0.0 { new.h / old.h } else { 1.0 });
                if let Shape::Pen { points } | Shape::Highlight { points } = shape {
                    for q in points.iter_mut() {
                        *q = Pt::new(new.x + (q.x - old.x) * sx, new.y + (q.y - old.y) * sy);
                    }
                }
            }
            (shape, h) => {
                let square = matches!(shape, Shape::Magnify { .. } | Shape::Stamp { .. });
                if let Some(rect) = shape.rect_mut() {
                    *rect = h.drag_rect(*rect, p, keep_aspect || square);
                }
            }
        }
    }

    /// Moves it by `d`.
    pub fn translate(&mut self, d: Pt) {
        let d = d.finite();
        self.map_points(&|p| p.add(d));
    }

    /// Every position in it passed through `f` (boxes keep two opposite corners and are
    /// normalized again, so a turn or a flip stays a box).
    pub fn map_points(&mut self, f: &dyn Fn(Pt) -> Pt) {
        match self {
            Shape::Arrow { from, to, .. } | Shape::Line { from, to } => {
                *from = f(*from).finite();
                *to = f(*to).finite();
            }
            Shape::Pen { points } | Shape::Highlight { points } => {
                for p in points.iter_mut() {
                    *p = f(*p).finite();
                }
            }
            Shape::Step { center, .. } => *center = f(*center).finite(),
            Shape::Callout { rect, tail, .. } => {
                *rect = Rect::from_points(f(rect.min()), f(rect.max()));
                *tail = f(*tail).finite();
            }
            other => {
                if let Some(rect) = other.rect_mut() {
                    *rect = Rect::from_points(f(rect.min()), f(rect.max()));
                }
            }
        }
    }

    /// The same shape with every coordinate finite and every list bounded, for anything read from
    /// outside.
    pub fn sanitized(mut self) -> Shape {
        self.map_points(&|p| p);
        match &mut self {
            Shape::Pen { points } | Shape::Highlight { points } => points.truncate(MAX_POINTS),
            Shape::Text { text, .. } | Shape::Callout { text, .. } => truncate_text(text, MAX_TEXT),
            Shape::Step { number, .. } => *number = (*number).min(9_999),
            _ => {}
        }
        self
    }
}

/// The most points a freehand line keeps.
pub const MAX_POINTS: usize = 100_000;
/// The most bytes of text an annotation keeps.
pub const MAX_TEXT: usize = 64 * 1024;

fn truncate_text(text: &mut String, max: usize) {
    if text.len() > max {
        let mut cut = max;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
    }
}

fn arrow_reach(shape: &Shape, style: &Style) -> f32 {
    match shape {
        Shape::Arrow { heads, .. } if *heads != ArrowHeads::None => arrow_head(style.width).1 * 0.8,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> Style {
        Style::default()
    }

    #[test]
    fn json_shape_is_tagged_and_flat() {
        let o = Object {
            id: ObjectId(3),
            shape: Shape::Arrow { from: Pt::new(1.0, 2.0), to: Pt::new(3.0, 4.0), heads: ArrowHeads::Both },
            style: style(),
        };
        let v = serde_json::to_value(&o).unwrap();
        assert_eq!(v["type"], "arrow");
        assert_eq!(v["id"], 3);
        assert_eq!(v["heads"], "both");
        assert_eq!(v["style"]["stroke"], "#E53935");
        let back: Object = serde_json::from_value(v).unwrap();
        assert_eq!(back, o);
        // Missing style and optional fields take their defaults.
        let minimal: Object = serde_json::from_str(r#"{"id":1,"type":"rect","rect":{"x":0,"y":0,"w":5,"h":5}}"#).unwrap();
        assert_eq!(minimal.shape, Shape::Rect { rect: Rect::new(0.0, 0.0, 5.0, 5.0), radius: 0.0 });
        assert!(serde_json::from_str::<Object>(r#"{"id":1,"type":"teapot"}"#).is_err());
    }

    #[test]
    fn hit_testing() {
        let s = style();
        let line = Shape::Line { from: Pt::new(0.0, 0.0), to: Pt::new(100.0, 0.0) };
        assert!(line.hit(&s, Pt::new(50.0, 3.0), 2.0));
        assert!(!line.hit(&s, Pt::new(50.0, 10.0), 2.0));
        let rect = Shape::Rect { rect: Rect::new(0.0, 0.0, 100.0, 100.0), radius: 0.0 };
        assert!(rect.hit(&s, Pt::new(1.0, 50.0), 2.0));
        assert!(!rect.hit(&s, Pt::new(50.0, 50.0), 2.0), "an unfilled box is hit on its border");
        let filled = Style { fill: Color::WHITE, ..s };
        assert!(rect.hit(&filled, Pt::new(50.0, 50.0), 2.0));
        let ell = Shape::Ellipse { rect: Rect::new(0.0, 0.0, 100.0, 50.0) };
        assert!(ell.hit(&s, Pt::new(100.0, 25.0), 2.0));
        assert!(!ell.hit(&s, Pt::new(50.0, 25.0), 2.0));
        let step = Shape::Step { center: Pt::new(10.0, 10.0), number: 1 };
        assert!(step.hit(&Kind::Step.default_style(), Pt::new(20.0, 10.0), 0.0));
        let pen = Shape::Pen { points: vec![Pt::new(0.0, 0.0), Pt::new(10.0, 10.0)] };
        assert!(pen.hit(&s, Pt::new(5.0, 5.0), 0.0));
        assert!(!Shape::Pen { points: vec![] }.hit(&s, Pt::ZERO, 10.0));
    }

    #[test]
    fn handles_drag_boxes_and_ends() {
        let s = style();
        let mut r = Shape::Rect { rect: Rect::new(10.0, 10.0, 20.0, 10.0), radius: 0.0 };
        r.drag_handle(&s, Handle::BottomRight, Pt::new(50.0, 40.0), false);
        assert_eq!(r.rect(), Some(Rect::new(10.0, 10.0, 40.0, 30.0)));
        // Dragging past the opposite side turns the box round rather than making it negative.
        r.drag_handle(&s, Handle::Left, Pt::new(60.0, 0.0), false);
        assert_eq!(r.rect(), Some(Rect::new(50.0, 10.0, 10.0, 30.0)));
        let mut a = Shape::Arrow { from: Pt::ZERO, to: Pt::new(5.0, 5.0), heads: ArrowHeads::End };
        a.drag_handle(&s, Handle::End, Pt::new(9.0, 1.0), false);
        assert_eq!(a, Shape::Arrow { from: Pt::ZERO, to: Pt::new(9.0, 1.0), heads: ArrowHeads::End });
        // Keep aspect on a corner.
        let r = Handle::BottomRight.drag_rect(Rect::new(0.0, 0.0, 20.0, 10.0), Pt::new(40.0, 5.0), true);
        assert_eq!(r, Rect::new(0.0, 0.0, 40.0, 20.0));
        // A freehand line scales with its box.
        let mut pen = Shape::Pen { points: vec![Pt::new(0.0, 0.0), Pt::new(10.0, 10.0)] };
        let thin = Style { width: 0.0, ..s };
        pen.drag_handle(&thin, Handle::BottomRight, Pt::new(20.0, 20.0), false);
        assert_eq!(pen, Shape::Pen { points: vec![Pt::new(0.0, 0.0), Pt::new(20.0, 20.0)] });
    }

    #[test]
    fn mapping_keeps_boxes_boxes() {
        let mut r = Shape::Blur { rect: Rect::new(10.0, 20.0, 30.0, 40.0) };
        // A quarter turn clockwise in a 100-tall picture: (x, y) -> (100 - y, x).
        r.map_points(&|p| Pt::new(100.0 - p.y, p.x));
        assert_eq!(r.rect(), Some(Rect::new(40.0, 10.0, 40.0, 30.0)));
        let mut t = Shape::Text { rect: Rect::default(), text: "é".repeat(MAX_TEXT) };
        t = t.sanitized();
        assert!(t.text().unwrap().len() <= MAX_TEXT);
        let nan = Shape::Line { from: Pt::new(f32::NAN, 1.0), to: Pt::new(f32::INFINITY, 2.0) }.sanitized();
        assert_eq!(nan, Shape::Line { from: Pt::new(0.0, 1.0), to: Pt::new(0.0, 2.0) });
    }

    #[test]
    fn styles_clamp_and_scale() {
        let s = Style { width: f32::NAN, opacity: 4.0, font_size: 1.0, ..Style::default() }.clamped();
        assert_eq!((s.width, s.opacity, s.font_size), (4.0, 1.0, MIN_FONT));
        let s = Style::default().scaled(2.0);
        assert_eq!((s.width, s.font_size), (8.0, 48.0));
        assert_eq!(Style::default().scaled(f32::NAN), Style::default());
        for k in Kind::ALL {
            let s = k.default_style();
            assert_eq!(s, s.clamped(), "{k:?}'s default is in range");
        }
    }
}
