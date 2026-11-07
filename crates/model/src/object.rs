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

    /// The colour a stamp is filled with when it is first placed.
    pub fn color(self) -> Color {
        match self {
            StampKind::Check => Color::rgb(0x2E, 0x9E, 0x4F),
            StampKind::Cross | StampKind::Heart => Color::rgb(0xE5, 0x39, 0x35),
            StampKind::Star => Color::rgb(0xFF, 0xB3, 0x00),
            StampKind::Question | StampKind::Info => Color::rgb(0x1E, 0x88, 0xE5),
            StampKind::Warning => Color::rgb(0xFB, 0x8C, 0x00),
        }
    }

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

/// A callout's outline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CalloutForm {
    /// A box with rounded corners.
    #[default]
    Rounded,
    /// A box with square corners.
    Square,
    /// An oval.
    Oval,
    /// A cloud, its tail a trail of bubbles: a thought.
    Cloud,
    /// A box that narrows to a point at its tail: an arrow with the text in it.
    Arrow,
}

impl CalloutForm {
    pub const ALL: [CalloutForm; 5] = [CalloutForm::Rounded, CalloutForm::Square, CalloutForm::Oval, CalloutForm::Cloud, CalloutForm::Arrow];

    pub fn label(self) -> &'static str {
        match self {
            CalloutForm::Rounded => "Rounded box",
            CalloutForm::Square => "Box",
            CalloutForm::Oval => "Oval",
            CalloutForm::Cloud => "Cloud",
            CalloutForm::Arrow => "Arrow",
        }
    }

    /// How much of the box's width and height the text gets: all of a box, the middle of an
    /// oval or a cloud (the square that fits in it).
    fn text_share(self) -> f32 {
        match self {
            CalloutForm::Oval => 0.72,
            CalloutForm::Cloud => 0.66,
            _ => 1.0,
        }
    }

    /// Where the text goes in a callout of this form drawn in `rect`.
    pub fn text_area(self, rect: Rect) -> Rect {
        let f = self.text_share();
        if f >= 1.0 {
            return rect;
        }
        let (w, h) = (rect.w * f, rect.h * f);
        let c = rect.center();
        Rect::new(c.x - w / 2.0, c.y - h / 2.0, w, h)
    }

    /// How tall the box must be for its text area to be `inner` tall.
    pub fn box_height(self, inner: f32) -> f32 {
        inner / self.text_share()
    }

    /// How far a cloud's bumps reach out of its box.
    pub fn bulge(self, rect: Rect) -> f32 {
        if self == CalloutForm::Cloud { (rect.w.min(rect.h) * 0.12).clamp(3.0, 24.0) } else { 0.0 }
    }
}

/// Where a callout's tail points from its box: what a new callout, or a quick style, starts with.
/// The tail can still be dragged anywhere afterwards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TailSpot {
    #[default]
    DownLeft,
    DownRight,
    UpLeft,
    UpRight,
    Left,
    Right,
    Up,
    Down,
}

impl TailSpot {
    pub const ALL: [TailSpot; 8] =
        [TailSpot::DownLeft, TailSpot::DownRight, TailSpot::UpLeft, TailSpot::UpRight, TailSpot::Left, TailSpot::Right, TailSpot::Up, TailSpot::Down];

    pub fn label(self) -> &'static str {
        match self {
            TailSpot::DownLeft => "Down left",
            TailSpot::DownRight => "Down right",
            TailSpot::UpLeft => "Up left",
            TailSpot::UpRight => "Up right",
            TailSpot::Left => "Left",
            TailSpot::Right => "Right",
            TailSpot::Up => "Up",
            TailSpot::Down => "Down",
        }
    }

    /// The tail's tip for a callout `w` by `h`, from the box's top left corner.
    pub fn offset(self, form: CalloutForm, w: f32, h: f32) -> Pt {
        let (w, h) = (w.max(0.0), h.max(0.0));
        // An arrow's point is short and stout; a speech tail reaches further.
        let reach = if form == CalloutForm::Arrow { (h * 0.5).max(8.0) } else { (h * 0.7).clamp(24.0, 90.0) };
        match self {
            TailSpot::DownLeft => Pt::new(w * 0.06, h + reach),
            TailSpot::DownRight => Pt::new(w * 0.94, h + reach),
            TailSpot::UpLeft => Pt::new(w * 0.06, -reach),
            TailSpot::UpRight => Pt::new(w * 0.94, -reach),
            TailSpot::Left => Pt::new(-reach, h / 2.0),
            TailSpot::Right => Pt::new(w + reach, h / 2.0),
            TailSpot::Up => Pt::new(w / 2.0, -reach),
            TailSpot::Down => Pt::new(w / 2.0, h + reach),
        }
    }

    /// The tail's tip for a callout drawn in `rect`.
    pub fn place(self, form: CalloutForm, rect: Rect) -> Pt {
        rect.min().add(self.offset(form, rect.w, rect.h)).finite()
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
    /// A speech balloon: a box (rounded, square, an oval, a cloud or an arrow) with a tail
    /// pointing at something, and text in it.
    Callout {
        rect: Rect,
        tail: Pt,
        text: String,
        #[serde(default)]
        form: CalloutForm,
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

    /// The tool a kind is drawn with: rectangles and ellipses are both the Shape tool's, and
    /// share its looks.
    pub fn family(self) -> Kind {
        match self {
            Kind::Ellipse => Kind::Rect,
            k => k,
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

/// The largest corner radius a rectangle is given from outside.
pub const MAX_RADIUS: f32 = 400.0;

/// What a tool, or a quick style, sets about a shape besides its style: an arrow's heads, a
/// rectangle's corners, a callout's form and tail, the stamp, a round spotlight or an ellipse.
/// What is none is left as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ShapeOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heads: Option<ArrowHeads>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<CalloutForm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail: Option<TailSpot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stamp: Option<StampKind>,
    /// Round rather than square: an ellipse rather than a rectangle (and the other way), a round
    /// spotlight.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round: Option<bool>,
}

impl ShapeOptions {
    /// The same options with every number in its range (NaN is dropped).
    pub fn clamped(self) -> ShapeOptions {
        ShapeOptions { radius: self.radius.filter(|r| r.is_finite()).map(|r| r.clamp(0.0, MAX_RADIUS)), ..self }
    }

    /// Whether these options say nothing.
    pub fn is_empty(&self) -> bool {
        *self == ShapeOptions::default()
    }

    /// `shape` with what these options say about its kind set; the rest of it untouched.
    pub fn applied(&self, shape: Shape) -> Shape {
        let o = self.clamped();
        match shape {
            Shape::Arrow { from, to, heads } => Shape::Arrow { from, to, heads: o.heads.unwrap_or(heads) },
            Shape::Rect { rect, .. } if o.round == Some(true) => Shape::Ellipse { rect },
            Shape::Rect { rect, radius } => Shape::Rect { rect, radius: o.radius.unwrap_or(radius) },
            Shape::Ellipse { rect } if o.round == Some(false) => Shape::Rect { rect, radius: o.radius.unwrap_or(0.0) },
            Shape::Callout { rect, tail, text, form } => {
                let form = o.form.unwrap_or(form);
                let tail = o.tail.map(|s| s.place(form, rect)).unwrap_or(tail);
                Shape::Callout { rect, tail, text, form }
            }
            Shape::Stamp { rect, stamp } => Shape::Stamp { rect, stamp: o.stamp.unwrap_or(stamp) },
            Shape::Spotlight { rect, ellipse } => Shape::Spotlight { rect, ellipse: o.round.unwrap_or(ellipse) },
            other => other,
        }
    }

    /// What `shape` itself says of these options (the tail's spot isn't kept in a shape).
    pub fn of(shape: &Shape) -> ShapeOptions {
        let none = ShapeOptions::default();
        match shape {
            Shape::Arrow { heads, .. } => ShapeOptions { heads: Some(*heads), ..none },
            Shape::Rect { radius, .. } => ShapeOptions { radius: Some(*radius), round: Some(false), ..none },
            Shape::Ellipse { .. } => ShapeOptions { round: Some(true), ..none },
            Shape::Callout { form, .. } => ShapeOptions { form: Some(*form), ..none },
            Shape::Stamp { stamp, .. } => ShapeOptions { stamp: Some(*stamp), ..none },
            Shape::Spotlight { ellipse, .. } => ShapeOptions { round: Some(*ellipse), ..none },
            _ => none,
        }
    }

    /// These options with `other`'s set over them.
    pub fn merged(self, other: ShapeOptions) -> ShapeOptions {
        ShapeOptions {
            heads: other.heads.or(self.heads),
            radius: other.radius.or(self.radius),
            form: other.form.or(self.form),
            tail: other.tail.or(self.tail),
            stamp: other.stamp.or(self.stamp),
            round: other.round.or(self.round),
        }
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
            Shape::Callout { rect, tail, form, .. } => rect.inflate(form.bulge(*rect)).union(Rect::new(tail.x, tail.y, 0.0, 0.0)).inflate(half),
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
    fn callouts_from_before_forms_read_as_rounded_boxes() {
        let old = r#"{"id":1,"type":"callout","rect":{"x":0,"y":0,"w":100,"h":40},"tail":{"x":10,"y":90},"text":"hi"}"#;
        let o: Object = serde_json::from_str(old).unwrap();
        let Shape::Callout { form, .. } = o.shape else { panic!("a callout") };
        assert_eq!(form, CalloutForm::Rounded);
        let arrow =
            Shape::Callout { rect: Rect::new(0.0, 0.0, 100.0, 40.0), tail: Pt::new(150.0, 20.0), text: String::new(), form: CalloutForm::Arrow };
        let v = serde_json::to_value(&arrow).unwrap();
        assert_eq!(v["form"], "arrow");
        assert_eq!(serde_json::from_value::<Shape>(v).unwrap(), arrow);
    }

    #[test]
    fn tail_spots_point_where_they_say() {
        let r = Rect::new(100.0, 100.0, 200.0, 80.0);
        let c = r.center();
        for spot in TailSpot::ALL {
            for form in CalloutForm::ALL {
                let p = spot.place(form, r);
                assert!(!r.contains(p), "{spot:?} {form:?}: the tail is outside the box");
                let (dx, dy) = (p.x - c.x, p.y - c.y);
                match spot {
                    TailSpot::DownLeft => assert!(dx < 0.0 && dy > 0.0),
                    TailSpot::DownRight => assert!(dx > 0.0 && dy > 0.0),
                    TailSpot::UpLeft => assert!(dx < 0.0 && dy < 0.0),
                    TailSpot::UpRight => assert!(dx > 0.0 && dy < 0.0),
                    TailSpot::Left => assert!(dx < 0.0 && dy.abs() < 0.5),
                    TailSpot::Right => assert!(dx > 0.0 && dy.abs() < 0.5),
                    TailSpot::Up => assert!(dy < 0.0 && dx.abs() < 0.5),
                    TailSpot::Down => assert!(dy > 0.0 && dx.abs() < 0.5),
                }
            }
        }
        // An arrow's point is shorter than a speech tail.
        let speech = TailSpot::Right.place(CalloutForm::Rounded, r);
        let arrow = TailSpot::Right.place(CalloutForm::Arrow, r);
        assert!(arrow.x < speech.x, "{arrow:?} {speech:?}");
        // Nonsense sizes still give a finite point.
        let p = TailSpot::Down.place(CalloutForm::Cloud, Rect::new(f32::NAN, 0.0, f32::INFINITY, -4.0));
        assert!(p.x.is_finite() && p.y.is_finite());
    }

    #[test]
    fn ovals_and_clouds_keep_their_text_in_the_middle() {
        let r = Rect::new(0.0, 0.0, 200.0, 100.0);
        assert_eq!(CalloutForm::Rounded.text_area(r), r);
        assert_eq!(CalloutForm::Arrow.text_area(r), r);
        let oval = CalloutForm::Oval.text_area(r);
        assert!(r.encloses(oval) && oval.w < r.w && oval.center() == r.center(), "{oval:?}");
        let h = CalloutForm::Cloud.box_height(50.0);
        assert!((CalloutForm::Cloud.text_area(Rect::new(0.0, 0.0, 100.0, h)).h - 50.0).abs() < 0.01);
        // A cloud's bumps are in its bounds.
        let cloud = Shape::Callout { rect: r, tail: Pt::new(100.0, 50.0), text: String::new(), form: CalloutForm::Cloud };
        assert!(cloud.bounds(&Style { width: 0.0, ..style() }).w > r.w);
    }

    #[test]
    fn shape_options_set_only_what_they_say() {
        let r = Rect::new(10.0, 10.0, 100.0, 50.0);
        let callout = Shape::Callout { rect: r, tail: Pt::new(0.0, 0.0), text: "hi".into(), form: CalloutForm::Rounded };
        let o =
            ShapeOptions { form: Some(CalloutForm::Arrow), tail: Some(TailSpot::Right), heads: Some(ArrowHeads::Both), ..ShapeOptions::default() };
        let Shape::Callout { form, tail, text, rect } = o.applied(callout) else { panic!("still a callout") };
        assert_eq!((form, text.as_str(), rect), (CalloutForm::Arrow, "hi", r));
        assert_eq!(tail, TailSpot::Right.place(CalloutForm::Arrow, r));
        // Round turns a rectangle into an ellipse and back, keeping its box.
        let round = ShapeOptions { round: Some(true), ..ShapeOptions::default() };
        assert_eq!(round.applied(Shape::Rect { rect: r, radius: 4.0 }), Shape::Ellipse { rect: r });
        let square = ShapeOptions { round: Some(false), radius: Some(f32::NAN), ..ShapeOptions::default() };
        assert_eq!(square.applied(Shape::Ellipse { rect: r }), Shape::Rect { rect: r, radius: 0.0 });
        let huge = ShapeOptions { radius: Some(1e9), ..ShapeOptions::default() };
        assert_eq!(huge.applied(Shape::Rect { rect: r, radius: 0.0 }), Shape::Rect { rect: r, radius: MAX_RADIUS });
        // Nothing said, nothing changed; a line has no options.
        let line = Shape::Line { from: Pt::ZERO, to: Pt::new(5.0, 5.0) };
        assert_eq!(o.applied(line.clone()), line);
        assert!(ShapeOptions::default().is_empty() && !o.is_empty());
        // What a shape says of itself, and options merged.
        assert_eq!(ShapeOptions::of(&Shape::Ellipse { rect: r }).round, Some(true));
        let merged = ShapeOptions { heads: Some(ArrowHeads::End), ..ShapeOptions::default() }.merged(o);
        assert_eq!((merged.heads, merged.form), (Some(ArrowHeads::Both), Some(CalloutForm::Arrow)));
        // As JSON, only what's set is written.
        assert_eq!(serde_json::to_string(&round).unwrap(), r#"{"round":true}"#);
        assert_eq!(Kind::Ellipse.family(), Kind::Rect);
        assert_eq!(Kind::Callout.family(), Kind::Callout);
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
