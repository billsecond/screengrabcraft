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

/// What an arrow's heads look like.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeadShape {
    /// A filled point with a notch at its back.
    #[default]
    Notched,
    /// A filled triangle.
    Triangle,
    /// Two lines meeting at the point.
    Open,
    /// A round dot.
    Dot,
    /// A short line across the end.
    Bar,
}

impl HeadShape {
    pub const ALL: [HeadShape; 5] = [HeadShape::Notched, HeadShape::Triangle, HeadShape::Open, HeadShape::Dot, HeadShape::Bar];

    pub fn label(self) -> &'static str {
        match self {
            HeadShape::Notched => "Notched",
            HeadShape::Triangle => "Solid",
            HeadShape::Open => "Open V",
            HeadShape::Dot => "Dot",
            HeadShape::Bar => "Bar",
        }
    }
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
    Plus,
    Minus,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    NoEntry,
    Idea,
    Pin,
    Flag,
    Lock,
    Clock,
    Search,
    Bookmark,
    Gear,
    Home,
    Mail,
    Pencil,
    Happy,
    Laughing,
    Wink,
    Neutral,
    Sad,
    Surprised,
    Angry,
    Love,
    Cool,
}

impl StampKind {
    pub const ALL: [StampKind; 34] = [
        StampKind::Check,
        StampKind::Cross,
        StampKind::Star,
        StampKind::Question,
        StampKind::Warning,
        StampKind::Info,
        StampKind::Heart,
        StampKind::Plus,
        StampKind::Minus,
        StampKind::ArrowUp,
        StampKind::ArrowDown,
        StampKind::ArrowLeft,
        StampKind::ArrowRight,
        StampKind::NoEntry,
        StampKind::Idea,
        StampKind::Pin,
        StampKind::Flag,
        StampKind::Lock,
        StampKind::Clock,
        StampKind::Search,
        StampKind::Bookmark,
        StampKind::Gear,
        StampKind::Home,
        StampKind::Mail,
        StampKind::Pencil,
        StampKind::Happy,
        StampKind::Laughing,
        StampKind::Wink,
        StampKind::Neutral,
        StampKind::Sad,
        StampKind::Surprised,
        StampKind::Angry,
        StampKind::Love,
        StampKind::Cool,
    ];

    /// The stamps the quick styles offer at once (the rest are in the library).
    pub const FIRST: [StampKind; 7] =
        [StampKind::Check, StampKind::Cross, StampKind::Star, StampKind::Question, StampKind::Warning, StampKind::Info, StampKind::Heart];

    /// The colour a stamp is filled with when it is first placed.
    pub fn color(self) -> Color {
        const GREEN: Color = Color::rgb(0x2E, 0x9E, 0x4F);
        const RED: Color = Color::rgb(0xE5, 0x39, 0x35);
        const BLUE: Color = Color::rgb(0x1E, 0x88, 0xE5);
        const AMBER: Color = Color::rgb(0xFF, 0xB3, 0x00);
        const ORANGE: Color = Color::rgb(0xFB, 0x8C, 0x00);
        const FACE: Color = Color::rgb(0xFF, 0xC8, 0x3D);
        const GREY: Color = Color::rgb(0x54, 0x6E, 0x7A);
        match self {
            StampKind::Check | StampKind::Plus => GREEN,
            StampKind::Cross | StampKind::Heart | StampKind::NoEntry | StampKind::Pin | StampKind::Minus => RED,
            StampKind::Star | StampKind::Idea => AMBER,
            StampKind::Question | StampKind::Info | StampKind::ArrowUp | StampKind::ArrowDown | StampKind::ArrowLeft | StampKind::ArrowRight => BLUE,
            StampKind::Mail | StampKind::Bookmark | StampKind::Search => BLUE,
            StampKind::Warning | StampKind::Flag | StampKind::Pencil => ORANGE,
            StampKind::Lock | StampKind::Clock | StampKind::Gear | StampKind::Home => GREY,
            StampKind::Happy
            | StampKind::Laughing
            | StampKind::Wink
            | StampKind::Neutral
            | StampKind::Sad
            | StampKind::Surprised
            | StampKind::Love
            | StampKind::Cool => FACE,
            StampKind::Angry => Color::rgb(0xF4, 0x51, 0x1E),
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
            StampKind::Plus => "Plus",
            StampKind::Minus => "Minus",
            StampKind::ArrowUp => "Arrow up",
            StampKind::ArrowDown => "Arrow down",
            StampKind::ArrowLeft => "Arrow left",
            StampKind::ArrowRight => "Arrow right",
            StampKind::NoEntry => "No entry",
            StampKind::Idea => "Idea",
            StampKind::Pin => "Pin",
            StampKind::Flag => "Flag",
            StampKind::Lock => "Lock",
            StampKind::Clock => "Clock",
            StampKind::Search => "Search",
            StampKind::Bookmark => "Bookmark",
            StampKind::Gear => "Settings",
            StampKind::Home => "Home",
            StampKind::Mail => "Mail",
            StampKind::Pencil => "Pencil",
            StampKind::Happy => "Happy face",
            StampKind::Laughing => "Laughing face",
            StampKind::Wink => "Winking face",
            StampKind::Neutral => "Straight face",
            StampKind::Sad => "Sad face",
            StampKind::Surprised => "Surprised face",
            StampKind::Angry => "Angry face",
            StampKind::Love => "Face in love",
            StampKind::Cool => "Face with sunglasses",
        }
    }

    /// Other words it is found by in the library's search.
    pub fn keywords(self) -> &'static str {
        match self {
            StampKind::Check => "yes done ok correct tick right good",
            StampKind::Cross => "no wrong error bad close x",
            StampKind::Star => "favourite favorite best rating important",
            StampKind::Question => "help ask unknown what",
            StampKind::Warning => "caution alert danger careful attention",
            StampKind::Info => "information note tip about",
            StampKind::Heart => "love like favourite",
            StampKind::Plus => "add more new",
            StampKind::Minus => "remove less take away",
            StampKind::ArrowUp => "up top higher increase direction",
            StampKind::ArrowDown => "down bottom lower decrease direction",
            StampKind::ArrowLeft => "left back previous direction",
            StampKind::ArrowRight => "right next forward direction",
            StampKind::NoEntry => "stop forbidden not allowed blocked",
            StampKind::Idea => "light bulb tip hint think",
            StampKind::Pin => "location place map marker here",
            StampKind::Flag => "mark goal finish report",
            StampKind::Lock => "secure private password locked safe",
            StampKind::Clock => "time wait later hour schedule",
            StampKind::Search => "find magnifier look zoom",
            StampKind::Bookmark => "save keep remember read later",
            StampKind::Gear => "gear cog options preferences setup",
            StampKind::Home => "house start main",
            StampKind::Mail => "email envelope message letter send",
            StampKind::Pencil => "edit write change draw",
            StampKind::Happy => "smile smiley glad emoji",
            StampKind::Laughing => "laugh joy funny grin emoji",
            StampKind::Wink => "wink joke playful emoji",
            StampKind::Neutral => "meh neutral blank emoji",
            StampKind::Sad => "unhappy sorry frown emoji",
            StampKind::Surprised => "wow shock amazed emoji",
            StampKind::Angry => "mad cross annoyed emoji",
            StampKind::Love => "heart eyes adore emoji",
            StampKind::Cool => "sunglasses cool emoji",
        }
    }

    /// Whether it is a face.
    pub fn is_face(self) -> bool {
        matches!(
            self,
            StampKind::Happy
                | StampKind::Laughing
                | StampKind::Wink
                | StampKind::Neutral
                | StampKind::Sad
                | StampKind::Surprised
                | StampKind::Angry
                | StampKind::Love
                | StampKind::Cool
        )
    }

    /// Whether `query` finds it: every word of the query begins a word of its name or keywords
    /// (any case). An empty query finds them all.
    pub fn matches(self, query: &str) -> bool {
        let words: Vec<String> = format!("{} {}", self.label(), self.keywords()).to_lowercase().split_whitespace().map(str::to_string).collect();
        query.to_lowercase().split_whitespace().all(|q| words.iter().any(|w| w.starts_with(q)))
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

/// How a step badge writes its place in the count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepLabels {
    /// 1, 2, 3
    #[default]
    Numbers,
    /// A, B, C … Z, AA, AB
    Upper,
    /// a, b, c … z, aa, ab
    Lower,
    /// I, II, III, IV
    Roman,
    /// i, ii, iii, iv
    RomanLower,
}

impl StepLabels {
    pub const ALL: [StepLabels; 5] = [StepLabels::Numbers, StepLabels::Upper, StepLabels::Lower, StepLabels::Roman, StepLabels::RomanLower];

    /// The first three, as the panel offers them.
    pub fn label(self) -> &'static str {
        match self {
            StepLabels::Numbers => "1, 2, 3",
            StepLabels::Upper => "A, B, C",
            StepLabels::Lower => "a, b, c",
            StepLabels::Roman => "I, II, III",
            StepLabels::RomanLower => "i, ii, iii",
        }
    }

    /// What they are called, for a tooltip.
    pub fn name(self) -> &'static str {
        match self {
            StepLabels::Numbers => "Numbers",
            StepLabels::Upper => "Capital letters",
            StepLabels::Lower => "Small letters",
            StepLabels::Roman => "Roman numerals",
            StepLabels::RomanLower => "Small Roman numerals",
        }
    }

    /// What the badge numbered `n` shows. Nought is 0 whatever the labels, and Roman numerals
    /// past 3999 (which they have no way to write) are figures.
    pub fn text(self, n: u32) -> String {
        if n == 0 {
            return "0".to_string();
        }
        match self {
            StepLabels::Numbers => n.to_string(),
            StepLabels::Upper => letters(n),
            StepLabels::Lower => letters(n).to_lowercase(),
            StepLabels::Roman => roman(n).unwrap_or_else(|| n.to_string()),
            StepLabels::RomanLower => roman(n).map_or_else(|| n.to_string(), |r| r.to_lowercase()),
        }
    }
}

/// A count in letters, as columns of a sheet are: A … Z, AA … AZ, BA …
fn letters(n: u32) -> String {
    let mut n = n;
    let mut out = Vec::new();
    while n > 0 && out.len() < 8 {
        n -= 1;
        out.push(char::from(b'A' + (n % 26) as u8));
        n /= 26;
    }
    out.iter().rev().collect()
}

/// A count in Roman numerals, 1 to 3999.
fn roman(n: u32) -> Option<String> {
    if !(1..=3999).contains(&n) {
        return None;
    }
    const PARTS: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut n = n;
    let mut out = String::new();
    for (value, part) in PARTS {
        while n >= value {
            out.push_str(part);
            n -= value;
        }
    }
    Some(out)
}

/// A step badge's outline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BadgeShape {
    #[default]
    Circle,
    /// A square with round corners.
    Rounded,
    Square,
    Diamond,
    Hexagon,
    /// A circle with a point under it, as a pin on a map: it points at what's below it.
    Pin,
}

impl BadgeShape {
    pub const ALL: [BadgeShape; 6] =
        [BadgeShape::Circle, BadgeShape::Rounded, BadgeShape::Square, BadgeShape::Diamond, BadgeShape::Hexagon, BadgeShape::Pin];

    pub fn label(self) -> &'static str {
        match self {
            BadgeShape::Circle => "Circle",
            BadgeShape::Rounded => "Rounded square",
            BadgeShape::Square => "Square",
            BadgeShape::Diamond => "Diamond",
            BadgeShape::Hexagon => "Hexagon",
            BadgeShape::Pin => "Pin",
        }
    }

    /// How far a badge of radius `r` reaches from its middle (where its label is): to each side,
    /// up and down. Each is about as heavy as a circle of that radius.
    pub fn reach(self, r: f32) -> (f32, f32, f32) {
        match self {
            BadgeShape::Circle => (r, r, r),
            BadgeShape::Rounded | BadgeShape::Square => (r * 0.9, r * 0.9, r * 0.9),
            BadgeShape::Diamond => (r * 1.25, r * 1.25, r * 1.25),
            // A hexagon standing on a point: its sides are upright.
            BadgeShape::Hexagon => (r * 1.1 * 0.866, r * 1.1, r * 1.1),
            BadgeShape::Pin => (r, r, r * PIN_POINT),
        }
    }

    /// How wide its label may be, for a badge of radius `r`: what fits across its middle.
    pub fn label_room(self, r: f32) -> f32 {
        match self {
            BadgeShape::Rounded | BadgeShape::Square => r * 1.35,
            BadgeShape::Diamond => r * 1.3,
            BadgeShape::Hexagon => r * 1.4,
            BadgeShape::Circle | BadgeShape::Pin => r * 1.5,
        }
    }

    /// The box round a badge of radius `r` at `center`.
    pub fn bounds(self, center: Pt, r: f32) -> Rect {
        let (side, up, down) = self.reach(r);
        Rect::new(center.x - side, center.y - up, 2.0 * side, up + down)
    }

    /// Whether `p` is on a badge of radius `r` at `center`, within `reach`.
    pub fn hit(self, center: Pt, r: f32, p: Pt, reach: f32) -> bool {
        let (dx, dy) = (p.x - center.x, p.y - center.y);
        match self {
            BadgeShape::Circle => p.distance(center) <= r + reach,
            BadgeShape::Diamond => {
                let half = self.reach(r).0;
                dx.abs() + dy.abs() <= half + reach * std::f32::consts::SQRT_2
            }
            // The pin's round head, and its point.
            BadgeShape::Pin => {
                p.distance(center) <= r + reach || (self.bounds(center, r).inflate(reach).contains(p) && dy > 0.0 && dx.abs() <= r * 0.6 + reach)
            }
            _ => self.bounds(center, r).inflate(reach).contains(p),
        }
    }
}

/// How far below a pin's middle its point is, in radii.
pub const PIN_POINT: f32 = 1.9;

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
        /// How far it bows out of a straight line at its middle, as a share of its length: to
        /// the left of the way it points (on the picture), or to the right when below nought.
        /// Nought is straight, and isn't written.
        #[serde(default, skip_serializing_if = "is_straight")]
        bend: f32,
        /// What its heads look like.
        #[serde(default, skip_serializing_if = "is_notched")]
        head: HeadShape,
    },
    Line {
        from: Pt,
        to: Pt,
        /// How far it bows out of a straight line, as an arrow's `bend`. Nought isn't written.
        #[serde(default, skip_serializing_if = "is_straight")]
        bend: f32,
    },
    Rect {
        rect: Rect,
        #[serde(default)]
        radius: f32,
    },
    Ellipse {
        rect: Rect,
    },
    /// A polygon with `sides` corners standing on its base (a triangle, a diamond, a pentagon…),
    /// or a star with `sides` points, filling its box.
    Polygon {
        rect: Rect,
        #[serde(default = "three")]
        sides: u32,
        #[serde(default)]
        star: bool,
        /// The corners of a polygon of points clicked one by one, as shares of its box (0 to 1
        /// across and down), so it is moved, resized and turned as a box is. None for a polygon
        /// of `sides` corners.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        corners: Vec<Pt>,
    },
    /// A freehand line.
    Pen {
        points: Vec<Pt>,
    },
    /// A see-through marker over a box, multiplied into the picture under it (its colour is the
    /// style's line colour).
    Highlight {
        #[serde(default)]
        rect: Rect,
        /// A stroke, as the highlighter drew before it drew boxes (0.1.51 and older), from an
        /// older file or a script: read, then made the box round it as the document takes it in
        /// ([`Shape::sanitized`]). Never written.
        #[serde(default, rename = "points", skip_serializing)]
        stroke: Vec<Pt>,
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
        /// Its other tails, pointing at other things (at most [`MAX_TAILS`] in all).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tails: Vec<Pt>,
    },
    /// A numbered badge, for steps in an explanation: its place in the count, written as a
    /// number, a letter or a Roman numeral, on a circle or another shape.
    Step {
        center: Pt,
        number: u32,
        #[serde(default)]
        labels: StepLabels,
        #[serde(default)]
        badge: BadgeShape,
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
    Polygon,
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
    pub const ALL: [Kind; 16] = [
        Kind::Arrow,
        Kind::Line,
        Kind::Rect,
        Kind::Ellipse,
        Kind::Polygon,
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
            Kind::Polygon => "Polygon",
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

    /// The tool a kind is drawn with: rectangles, ellipses and polygons are all the Shape
    /// tool's, and share its looks.
    pub fn family(self) -> Kind {
        match self {
            Kind::Ellipse | Kind::Polygon => Kind::Rect,
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
            Kind::Rect | Kind::Ellipse | Kind::Polygon => Style { width: 4.0, shadow: true, ..base },
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

/// The most bytes of a font's name kept.
pub const MAX_FONT_NAME: usize = 63;

/// A font on the computer, by its family's name ("Segoe UI", "DejaVu Serif"); empty for the
/// bundled fonts. Kept in a style (which is copied freely), so it is a short name held in place.
/// A project keeps the name; where that font is missing, the text is drawn in the bundled font.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontName {
    len: u8,
    bytes: [u8; MAX_FONT_NAME],
}

impl Default for FontName {
    fn default() -> FontName {
        FontName { len: 0, bytes: [0; MAX_FONT_NAME] }
    }
}

impl FontName {
    /// The bundled fonts.
    pub const BUNDLED: FontName = FontName { len: 0, bytes: [0; MAX_FONT_NAME] };

    /// `name`, trimmed, without control characters, cut to [`MAX_FONT_NAME`] bytes at a character's edge.
    pub fn new(name: &str) -> FontName {
        let mut out = FontName::default();
        let mut len = 0;
        for ch in name.trim().chars().filter(|c| !c.is_control()) {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf).as_bytes();
            let Some(dst) = out.bytes.get_mut(len..len + s.len()) else { break };
            dst.copy_from_slice(s);
            len += s.len();
        }
        out.len = len as u8;
        out
    }

    pub fn as_str(&self) -> &str {
        let bytes = self.bytes.get(..usize::from(self.len)).unwrap_or_default();
        std::str::from_utf8(bytes).unwrap_or_default()
    }

    /// Whether it is the bundled fonts (no name).
    pub fn is_bundled(&self) -> bool {
        self.len == 0
    }
}

impl std::fmt::Debug for FontName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl Serialize for FontName {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FontName {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<FontName, D::Error> {
        // Anything else than words is the bundled font, not an error: a style is never lost over it.
        let v = serde_json::Value::deserialize(d)?;
        Ok(v.as_str().map(FontName::new).unwrap_or_default())
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
    /// A line round each letter of the text, so it reads on any picture (transparent for none).
    pub text_outline: Color,
    /// How thick that line is, in pixels outside the letter.
    pub text_outline_width: f32,
    /// A font on the computer for the text, by name; the bundled fonts when none (or where the
    /// computer hasn't it).
    #[serde(skip_serializing_if = "FontName::is_bundled")]
    pub font: FontName,
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
            text_outline: Color::TRANSPARENT,
            text_outline_width: 3.0,
            font: FontName::BUNDLED,
        }
    }
}

/// The thickest a text outline is drawn.
pub const MAX_TEXT_OUTLINE: f32 = 40.0;

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
            text_outline_width: fix(self.text_outline_width, 0.0, MAX_TEXT_OUTLINE, d.text_outline_width),
            ..self
        }
    }

    /// Lengths in the style (line width, font size, text outline) scaled by `f`, for a picture
    /// resized by it.
    pub fn scaled(self, f: f32) -> Style {
        if !f.is_finite() || f <= 0.0 {
            return self;
        }
        Style { width: self.width * f, font_size: self.font_size * f, text_outline_width: self.text_outline_width * f, ..self }.clamped()
    }

    /// Whether its text has an outline drawn round it.
    pub fn has_text_outline(&self) -> bool {
        self.text_outline.a > 0 && self.text_outline_width > 0.0
    }
}

/// The largest corner radius a rectangle is given from outside.
pub const MAX_RADIUS: f32 = 400.0;

/// What a tool, or a quick style, sets about a shape besides its style: an arrow's heads, a
/// rectangle's corners, a callout's form and tail, the stamp, a round spotlight or an ellipse, a
/// step badge's labels and outline. What is none is left as it is.
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
    /// How a step badge writes its count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<StepLabels>,
    /// A step badge's outline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<BadgeShape>,
    /// How far an arrow bows out of a straight line (see [`Shape::Arrow`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bend: Option<f32>,
    /// A polygon's corners, or a star's points. Said of a rectangle or an ellipse (and not
    /// `round`), it makes it a polygon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sides: Option<u32>,
    /// A star rather than a polygon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub star: Option<bool>,
    /// What an arrow's heads look like.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<HeadShape>,
}

impl ShapeOptions {
    /// The same options with every number in its range (NaN is dropped).
    pub fn clamped(self) -> ShapeOptions {
        ShapeOptions {
            radius: self.radius.filter(|r| r.is_finite()).map(|r| r.clamp(0.0, MAX_RADIUS)),
            bend: self.bend.filter(|b| b.is_finite()).map(clamp_bend),
            sides: self.sides.map(|n| n.clamp(MIN_SIDES, MAX_SIDES)),
            ..self
        }
    }

    /// A polygon of these options in `rect`: their corners (three unless they say) and star.
    fn polygon(&self, rect: Rect, sides: u32, star: bool) -> Shape {
        Shape::Polygon { rect, sides: self.sides.unwrap_or(sides), star: self.star.unwrap_or(star), corners: Vec::new() }
    }

    /// Whether these options say nothing.
    pub fn is_empty(&self) -> bool {
        *self == ShapeOptions::default()
    }

    /// `shape` with what these options say about its kind set; the rest of it untouched.
    pub fn applied(&self, shape: Shape) -> Shape {
        let o = self.clamped();
        match shape {
            Shape::Arrow { from, to, heads, bend, head } => {
                Shape::Arrow { from, to, heads: o.heads.unwrap_or(heads), bend: o.bend.unwrap_or(bend), head: o.head.unwrap_or(head) }
            }
            // Round makes an ellipse, square a rectangle; corners (and neither) a polygon.
            Shape::Rect { rect, .. } | Shape::Polygon { rect, .. } if o.round == Some(true) => Shape::Ellipse { rect },
            Shape::Rect { rect, .. } | Shape::Ellipse { rect } if o.round.is_none() && o.sides.is_some() => o.polygon(rect, MIN_SIDES, false),
            Shape::Rect { rect, radius } => Shape::Rect { rect, radius: o.radius.unwrap_or(radius) },
            Shape::Ellipse { rect } | Shape::Polygon { rect, .. } if o.round == Some(false) => Shape::Rect { rect, radius: o.radius.unwrap_or(0.0) },
            // A polygon of clicked points stays one, unless its corners are set.
            Shape::Polygon { rect, sides, star, corners } if !corners.is_empty() && o.sides.is_none() => {
                Shape::Polygon { rect, sides, star, corners }
            }
            Shape::Polygon { rect, sides, star, .. } => o.polygon(rect, sides, star),
            Shape::Callout { rect, tail, text, form, tails } => {
                let form = o.form.unwrap_or(form);
                let tail = o.tail.map(|s| s.place(form, rect)).unwrap_or(tail);
                Shape::Callout { rect, tail, text, form, tails }
            }
            Shape::Line { from, to, bend } => Shape::Line { from, to, bend: o.bend.unwrap_or(bend) },
            Shape::Stamp { rect, stamp } => Shape::Stamp { rect, stamp: o.stamp.unwrap_or(stamp) },
            Shape::Spotlight { rect, ellipse } => Shape::Spotlight { rect, ellipse: o.round.unwrap_or(ellipse) },
            Shape::Step { center, number, labels, badge } => {
                Shape::Step { center, number, labels: o.labels.unwrap_or(labels), badge: o.badge.unwrap_or(badge) }
            }
            other => other,
        }
    }

    /// What `shape` itself says of these options (the tail's spot isn't kept in a shape).
    pub fn of(shape: &Shape) -> ShapeOptions {
        let none = ShapeOptions::default();
        match shape {
            Shape::Arrow { heads, bend, head, .. } => ShapeOptions { heads: Some(*heads), bend: Some(*bend), head: Some(*head), ..none },
            Shape::Rect { radius, .. } => ShapeOptions { radius: Some(*radius), round: Some(false), ..none },
            Shape::Ellipse { .. } => ShapeOptions { round: Some(true), ..none },
            Shape::Polygon { sides, star, .. } => ShapeOptions { sides: Some(*sides), star: Some(*star), ..none },
            Shape::Line { bend, .. } => ShapeOptions { bend: Some(*bend), ..none },
            Shape::Callout { form, .. } => ShapeOptions { form: Some(*form), ..none },
            Shape::Stamp { stamp, .. } => ShapeOptions { stamp: Some(*stamp), ..none },
            Shape::Spotlight { ellipse, .. } => ShapeOptions { round: Some(*ellipse), ..none },
            Shape::Step { labels, badge, .. } => ShapeOptions { labels: Some(*labels), badge: Some(*badge), ..none },
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
            labels: other.labels.or(self.labels),
            badge: other.badge.or(self.badge),
            bend: other.bend.or(self.bend),
            sides: other.sides.or(self.sides),
            star: other.star.or(self.star),
            head: other.head.or(self.head),
        }
    }
}

/// The size of an arrow's head for a line width: its length along the line and its half width.
pub fn arrow_head(width: f32) -> (f32, f32) {
    let w = width.max(1.0);
    (w * 3.0 + 8.0, w * 1.7 + 5.0)
}

/// The most an arrow bends: how far its middle bows out of the straight line, as a share of its
/// length.
pub const MAX_BEND: f32 = 1.5;
/// How many straight pieces a curved arrow is measured in (for its bounds and hit testing).
pub const CURVE_PIECES: usize = 48;
/// The fewest and most corners (or points, for a star) a polygon has.
pub const MIN_SIDES: u32 = 3;
pub const MAX_SIDES: u32 = 12;

fn three() -> u32 {
    3
}

fn is_straight(bend: &f32) -> bool {
    *bend == 0.0
}

fn is_notched(head: &HeadShape) -> bool {
    *head == HeadShape::Notched
}

/// A bend in range: finite, at most [`MAX_BEND`] either way.
pub fn clamp_bend(bend: f32) -> f32 {
    if bend.is_finite() { bend.clamp(-MAX_BEND, MAX_BEND) } else { 0.0 }
}

/// The side to the left of the way from `from` to `to` (on a picture, whose y grows downwards),
/// one long; none when the two are the same point.
fn left_of(from: Pt, to: Pt) -> Option<(Pt, f32)> {
    let d = to.sub(from);
    let len = d.length();
    if !(len > 0.001 && len.is_finite()) {
        return None;
    }
    Some((Pt::new(d.y / len, -d.x / len), len))
}

/// The middle of an arrow from `from` to `to` bowed by `bend`: where its bend handle is.
pub fn bend_middle(from: Pt, to: Pt, bend: f32) -> Pt {
    let mid = from.lerp(to, 0.5);
    match left_of(from, to) {
        Some((n, len)) => mid.add(n.scale(clamp_bend(bend) * len)).finite(),
        None => mid,
    }
}

/// The control point of the curve from `from` to `to` through [`bend_middle`].
pub fn bend_control(from: Pt, to: Pt, bend: f32) -> Pt {
    let mid = from.lerp(to, 0.5);
    let m = bend_middle(from, to, bend);
    m.scale(2.0).sub(mid).finite()
}

/// Points along an arrow bowed by `bend`, from `from` to `to`, in `pieces` straight pieces (just
/// the two ends when it is straight).
pub fn bent_points(from: Pt, to: Pt, bend: f32, pieces: usize) -> Vec<Pt> {
    if clamp_bend(bend) == 0.0 || left_of(from, to).is_none() {
        return vec![from, to];
    }
    let c = bend_control(from, to, bend);
    let n = pieces.clamp(2, 256);
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            Pt::new(u * u * from.x + 2.0 * u * t * c.x + t * t * to.x, u * u * from.y + 2.0 * u * t * c.y + t * t * to.y)
        })
        .collect()
}

/// How far from the straight line through `from` and `to` a point `p` is, to its left, as a share
/// of the line's length: the bend that puts the arrow's middle level with `p`.
pub fn bend_through(from: Pt, to: Pt, p: Pt) -> f32 {
    let Some((n, len)) = left_of(from, to) else { return 0.0 };
    let off = p.sub(from.lerp(to, 0.5));
    let along = off.x * n.x + off.y * n.y;
    // Close to straight is straight.
    if along.abs() < 3.0 { 0.0 } else { clamp_bend(along / len) }
}

/// The corners of a polygon with `sides` corners (a star with `sides` points) standing on its
/// base, made to fill `rect`.
pub fn polygon_points(rect: Rect, sides: u32, star: bool) -> Vec<Pt> {
    let n = sides.clamp(MIN_SIDES, MAX_SIDES) as usize;
    let inner = if n <= 5 { 0.4 } else { 0.58 };
    let count = if star { n * 2 } else { n };
    let step = std::f32::consts::TAU / count as f32;
    let unit: Vec<Pt> = (0..count)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * step;
            let r = if star && i % 2 == 1 { inner } else { 1.0 };
            Pt::new(a.cos() * r, a.sin() * r)
        })
        .collect();
    let b = Rect::bounding(&unit);
    let (sx, sy) = (if b.w > 0.0 { rect.w / b.w } else { 0.0 }, if b.h > 0.0 { rect.h / b.h } else { 0.0 });
    unit.iter().map(|p| Pt::new(rect.x + (p.x - b.x) * sx, rect.y + (p.y - b.y) * sy).finite()).collect()
}

/// The most corners a polygon of clicked points keeps.
pub const MAX_CORNERS: usize = 200;
/// The most tails a callout has.
pub const MAX_TAILS: usize = 8;

/// The corners of a polygon: its clicked points (shares of `rect`) when it has three or more,
/// else `sides` corners (or star points) filling `rect`.
pub fn polygon_outline(rect: Rect, sides: u32, star: bool, corners: &[Pt]) -> Vec<Pt> {
    if corners.len() < 3 {
        return polygon_points(rect, sides, star);
    }
    corners.iter().take(MAX_CORNERS).map(|c| Pt::new(rect.x + c.x * rect.w, rect.y + c.y * rect.h).finite()).collect()
}

/// A polygon through `points` (on the picture), clicked one by one: the box round them and each
/// as a share of it. None for fewer than three points or none apart.
pub fn polygon_through(points: &[Pt]) -> Option<Shape> {
    let points: Vec<Pt> = points.iter().take(MAX_CORNERS).map(|p| p.finite()).collect();
    if points.len() < 3 {
        return None;
    }
    let rect = Rect::bounding(&points);
    if rect.w < 1.0 && rect.h < 1.0 {
        return None;
    }
    let share = |v: f32, lo: f32, len: f32| if len > 0.0 { ((v - lo) / len).clamp(0.0, 1.0) } else { 0.5 };
    let corners = points.iter().map(|p| Pt::new(share(p.x, rect.x, rect.w), share(p.y, rect.y, rect.h))).collect();
    Some(Shape::Polygon { rect, sides: MIN_SIDES, star: false, corners })
}

/// Whether `p` is inside the closed outline through `points` (even-odd).
pub fn inside_polygon(p: Pt, points: &[Pt]) -> bool {
    let mut inside = false;
    let n = points.len();
    for i in 0..n {
        let (Some(a), Some(b)) = (points.get(i), points.get((i + n - 1) % n)) else { continue };
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x {
                inside = !inside;
            }
        }
    }
    inside
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
    /// A callout's other tails, by their place in its list of them.
    TailAt(u8),
    /// The middle of an arrow or a line, dragged to bend it.
    Bend,
    /// Above a box (or a selection), dragged round to turn it.
    Rotate,
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
            Handle::Start | Handle::End | Handle::Tail | Handle::TailAt(_) | Handle::Bend | Handle::Rotate => c,
        }
    }

    /// The corner or side across the box from it, which stays put while it is dragged.
    pub fn opposite(self) -> Option<Handle> {
        Some(match self {
            Handle::TopLeft => Handle::BottomRight,
            Handle::Top => Handle::Bottom,
            Handle::TopRight => Handle::BottomLeft,
            Handle::Right => Handle::Left,
            Handle::BottomRight => Handle::TopLeft,
            Handle::Bottom => Handle::Top,
            Handle::BottomLeft => Handle::TopRight,
            Handle::Left => Handle::Right,
            Handle::Start | Handle::End | Handle::Tail | Handle::TailAt(_) | Handle::Bend | Handle::Rotate => return None,
        })
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
            Handle::Start | Handle::End | Handle::Tail | Handle::TailAt(_) | Handle::Bend | Handle::Rotate => return r,
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
    /// How far it is turned round its middle, in degrees the way a clock goes (-180 to 180).
    /// Only boxes keep a turn of their own ([`Shape::turns`]): lines and strokes are turned by
    /// moving their points. Nought (and every annotation of an older file) isn't written.
    #[serde(default, skip_serializing_if = "is_unturned")]
    pub rotation: f32,
    /// The group it is in: a group is selected, moved, turned and lined up as one. None when it
    /// is on its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u64>,
}

fn is_unturned(r: &f32) -> bool {
    *r == 0.0
}

/// A turn in range: finite, -180 to 180 degrees; a hair's breadth from none is none.
pub fn clamp_rotation(degrees: f32) -> f32 {
    if !degrees.is_finite() {
        return 0.0;
    }
    let r = (degrees + 180.0).rem_euclid(360.0) - 180.0;
    if r.abs() < 0.01 { 0.0 } else { r }
}

/// `p` turned by `degrees` round `c`, the way a clock goes (on a picture, whose y grows
/// downwards).
pub fn turn(p: Pt, c: Pt, degrees: f32) -> Pt {
    if degrees == 0.0 || !degrees.is_finite() {
        return p;
    }
    let (s, co) = degrees.to_radians().sin_cos();
    let d = p.sub(c);
    Pt::new(c.x + d.x * co - d.y * s, c.y + d.x * s + d.y * co).finite()
}

/// The box round `r` turned by `degrees` round `c`.
pub fn turned_box(r: Rect, c: Pt, degrees: f32) -> Rect {
    if degrees == 0.0 {
        return r;
    }
    Rect::bounding(&r.corners().map(|q| turn(q, c, degrees)))
}

impl Object {
    pub fn new(id: ObjectId, shape: Shape, style: Style) -> Object {
        Object { id, shape, style, rotation: 0.0, group: None }
    }

    /// Its turn, when it is one that counts (a box's).
    pub fn turn(&self) -> f32 {
        if self.shape.turns() { clamp_rotation(self.rotation) } else { 0.0 }
    }

    /// The area it covers on the picture, turned, its line width included.
    pub fn bounds(&self) -> Rect {
        turned_box(self.shape.bounds(&self.style), self.shape.pivot(), self.turn())
    }

    /// The area its own points cover on the picture, turned.
    pub fn extent(&self) -> Rect {
        turned_box(self.shape.extent(), self.shape.pivot(), self.turn())
    }

    /// Whether `p` (on the picture) touches it, within `tol` pixels.
    pub fn hit(&self, p: Pt, tol: f32) -> bool {
        self.shape.hit(&self.style, turn(p, self.shape.pivot(), -self.turn()), tol)
    }

    /// Where it can be dragged by, on the picture.
    pub fn handles(&self) -> Vec<(Handle, Pt)> {
        let (c, a) = (self.shape.pivot(), self.turn());
        self.shape.handles(&self.style).into_iter().map(|(h, p)| (h, turn(p, c, a))).collect()
    }

    /// Moves `handle` to `p` (on the picture). A turned box is reshaped along its own sides,
    /// the corner or side across from the handle staying where it is.
    pub fn drag_handle(&mut self, handle: Handle, p: Pt, keep_aspect: bool) {
        let a = self.turn();
        if a == 0.0 {
            self.shape.drag_handle(&self.style, handle, p, keep_aspect);
            return;
        }
        let c = self.shape.pivot();
        let anchor = handle.opposite();
        let place = |shape: &Shape, style: &Style, h: Handle| shape.handles(style).into_iter().find(|(k, _)| *k == h).map(|(_, q)| q);
        let before = anchor.and_then(|h| place(&self.shape, &self.style, h)).map(|q| turn(q, c, a));
        self.shape.drag_handle(&self.style, handle, turn(p, c, -a), keep_aspect);
        if let (Some(before), Some(h)) = (before, anchor)
            && let Some(q) = place(&self.shape, &self.style, h)
        {
            let after = turn(q, self.shape.pivot(), a);
            self.shape.translate(before.sub(after));
        }
    }
}

impl Shape {
    pub fn kind(&self) -> Kind {
        match self {
            Shape::Arrow { .. } => Kind::Arrow,
            Shape::Line { .. } => Kind::Line,
            Shape::Rect { .. } => Kind::Rect,
            Shape::Ellipse { .. } => Kind::Ellipse,
            Shape::Polygon { .. } => Kind::Polygon,
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

    /// Whether it keeps a turn of its own: a box, but not one that changes the picture under it
    /// (blur, pixelate, spotlight, magnify stay square to the picture).
    pub fn turns(&self) -> bool {
        self.rect().is_some() && !self.kind().is_effect()
    }

    /// What it turns round: the middle of its box (a callout's box, not its tail), else of what
    /// it covers.
    pub fn pivot(&self) -> Pt {
        self.rect().unwrap_or_else(|| self.extent()).center()
    }

    /// Mirrored round `c`, across (`horizontal`) or up and down: its points move, and a stamp of an
    /// arrow points the other way. Words and pictures still read the right way round.
    pub fn mirror(&mut self, c: Pt, horizontal: bool) {
        if horizontal {
            self.map_points(&|p| Pt::new(2.0 * c.x - p.x, p.y));
        } else {
            self.map_points(&|p| Pt::new(p.x, 2.0 * c.y - p.y));
        }
        if let Shape::Stamp { stamp, .. } = self {
            *stamp = match (*stamp, horizontal) {
                (StampKind::ArrowLeft, true) => StampKind::ArrowRight,
                (StampKind::ArrowRight, true) => StampKind::ArrowLeft,
                (StampKind::ArrowUp, false) => StampKind::ArrowDown,
                (StampKind::ArrowDown, false) => StampKind::ArrowUp,
                (s, _) => s,
            };
        }
    }

    /// The box of a box-shaped annotation.
    pub fn rect(&self) -> Option<Rect> {
        match self {
            Shape::Rect { rect, .. }
            | Shape::Ellipse { rect }
            | Shape::Polygon { rect, .. }
            | Shape::Highlight { rect, .. }
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
            | Shape::Polygon { rect, .. }
            | Shape::Highlight { rect, .. }
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
            Shape::Arrow { from, to, heads, bend, .. } => {
                let grow = if *heads == ArrowHeads::None { half } else { arrow_head(style.width).1.max(half) };
                Rect::bounding(&bent_points(*from, *to, *bend, CURVE_PIECES)).inflate(grow)
            }
            Shape::Line { from, to, bend } => Rect::bounding(&bent_points(*from, *to, *bend, CURVE_PIECES)).inflate(half),
            Shape::Pen { points } => Rect::bounding(points).inflate(half),
            Shape::Step { center, badge, .. } => badge.bounds(*center, step_radius(style.font_size)).inflate(half),
            Shape::Callout { rect, tail, form, tails, .. } => {
                let mut r = rect.inflate(form.bulge(*rect));
                for t in std::iter::once(tail).chain(tails) {
                    r = r.union(Rect::new(t.x, t.y, 0.0, 0.0));
                }
                r.inflate(half)
            }
            // A highlight is its box, with no line round it.
            Shape::Spotlight { rect, .. } | Shape::Blur { rect } | Shape::Pixelate { rect } | Shape::Highlight { rect, .. } => *rect,
            _ => self.rect().unwrap_or_default().inflate(half),
        }
    }

    /// The area its own points cover, without its line width or arrow heads.
    pub fn extent(&self) -> Rect {
        match self {
            Shape::Arrow { from, to, bend, .. } => Rect::bounding(&bent_points(*from, *to, *bend, CURVE_PIECES)),
            Shape::Line { from, to, bend } => Rect::bounding(&bent_points(*from, *to, *bend, CURVE_PIECES)),
            Shape::Pen { points } => Rect::bounding(points),
            Shape::Step { center, .. } => Rect::new(center.x, center.y, 0.0, 0.0),
            Shape::Callout { rect, tail, tails, .. } => std::iter::once(tail).chain(tails).fold(*rect, |r, t| r.union(Rect::new(t.x, t.y, 0.0, 0.0))),
            _ => self.rect().unwrap_or_default(),
        }
    }

    /// Whether `p` touches it, within `tol` pixels.
    pub fn hit(&self, style: &Style, p: Pt, tol: f32) -> bool {
        let reach = style.width.max(0.0) / 2.0 + tol;
        match self {
            Shape::Arrow { from, to, bend, .. } => {
                distance_to_polyline(p, &bent_points(*from, *to, *bend, CURVE_PIECES)) <= reach.max(arrow_reach(self, style))
            }
            Shape::Line { from, to, bend } => distance_to_polyline(p, &bent_points(*from, *to, *bend, CURVE_PIECES)) <= reach,
            Shape::Pen { points } => distance_to_polyline(p, points) <= reach,
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
            Shape::Polygon { rect, sides, star, corners } => {
                let corners = polygon_outline(*rect, *sides, *star, corners);
                let mut ring = corners.clone();
                ring.extend(corners.first().copied());
                distance_to_polyline(p, &ring) <= reach || (!style.fill.is_transparent() && inside_polygon(p, &corners))
            }
            Shape::Step { center, badge, .. } => badge.hit(*center, step_radius(style.font_size), p, reach),
            Shape::Callout { rect, tail, tails, .. } => {
                rect.inflate(reach).contains(p)
                    || std::iter::once(tail).chain(tails).any(|t| distance_to_segment(p, rect.center(), *t) <= reach.max(6.0))
            }
            _ => self.rect().is_some_and(|r| r.inflate(tol).contains(p)),
        }
    }

    /// Where it can be dragged by.
    pub fn handles(&self, style: &Style) -> Vec<(Handle, Pt)> {
        match self {
            Shape::Arrow { from, to, bend, .. } => vec![(Handle::Start, *from), (Handle::End, *to), (Handle::Bend, bend_middle(*from, *to, *bend))],
            Shape::Line { from, to, bend } => vec![(Handle::Start, *from), (Handle::End, *to), (Handle::Bend, bend_middle(*from, *to, *bend))],
            Shape::Step { .. } => Vec::new(),
            Shape::Pen { .. } => {
                let b = self.bounds(style);
                Handle::BOX.iter().map(|h| (*h, h.on(b))).collect()
            }
            Shape::Callout { rect, tail, tails, .. } => {
                let mut all: Vec<_> = Handle::BOX.iter().map(|h| (*h, h.on(*rect))).collect();
                all.push((Handle::Tail, *tail));
                all.extend(tails.iter().take(MAX_TAILS).enumerate().map(|(i, t)| (Handle::TailAt(i as u8), *t)));
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
            (Shape::Arrow { from, to, bend, .. } | Shape::Line { from, to, bend }, Handle::Bend) => *bend = bend_through(*from, *to, p),
            (Shape::Callout { tail, .. }, Handle::Tail) => *tail = p,
            (Shape::Callout { tails, .. }, Handle::TailAt(i)) => {
                if let Some(t) = tails.get_mut(usize::from(i)) {
                    *t = p;
                }
            }
            (shape @ Shape::Pen { .. }, h) => {
                let old = shape.bounds(style).inflate(-style.width.max(0.0) / 2.0);
                let new = h.drag_rect(old, p, keep_aspect);
                let (sx, sy) = (if old.w > 0.0 { new.w / old.w } else { 1.0 }, if old.h > 0.0 { new.h / old.h } else { 1.0 });
                if let Shape::Pen { points } = shape {
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
            Shape::Arrow { from, to, bend, .. } | Shape::Line { from, to, bend } => {
                // A mirror (a flip) turns the bow the other way round the line; a turn doesn't.
                if let Some((n, _)) = left_of(*from, *to) {
                    let (a, b, side) = (f(*from), f(*to), f(from.add(n)));
                    let (d, s) = (b.sub(a), side.sub(a));
                    if d.x * s.y - d.y * s.x > 0.0 {
                        *bend = -*bend;
                    }
                }
                *from = f(*from).finite();
                *to = f(*to).finite();
            }
            Shape::Pen { points } => {
                for p in points.iter_mut() {
                    *p = f(*p).finite();
                }
            }
            Shape::Highlight { rect, stroke } => {
                for p in stroke.iter_mut() {
                    *p = f(*p).finite();
                }
                *rect = Rect::from_points(f(rect.min()), f(rect.max()));
            }
            Shape::Step { center, .. } => *center = f(*center).finite(),
            Shape::Callout { rect, tail, tails, .. } => {
                *rect = Rect::from_points(f(rect.min()), f(rect.max()));
                *tail = f(*tail).finite();
                for t in tails.iter_mut() {
                    *t = f(*t).finite();
                }
            }
            other => {
                if let Some(rect) = other.rect_mut() {
                    *rect = Rect::from_points(f(rect.min()), f(rect.max()));
                }
            }
        }
    }

    /// A highlight over `rect`.
    pub fn highlight(rect: Rect) -> Shape {
        Shape::Highlight { rect, stroke: Vec::new() }
    }

    /// A step badge numbered `number` at `center`: a number on a circle.
    pub fn step(center: Pt, number: u32) -> Shape {
        Shape::Step { center, number, labels: StepLabels::default(), badge: BadgeShape::default() }
    }

    /// What a step badge shows; none for anything else.
    pub fn step_label(&self) -> Option<String> {
        match self {
            Shape::Step { number, labels, .. } => Some(labels.text(*number)),
            _ => None,
        }
    }

    /// The same shape with every coordinate finite and every list bounded, for anything read from
    /// outside. A highlighter stroke of an older file is made the box round it, its line `width`
    /// (`style.width`) included.
    pub fn sanitized(mut self, width: f32) -> Shape {
        self.map_points(&|p| p);
        match &mut self {
            Shape::Pen { points } => points.truncate(MAX_POINTS),
            Shape::Highlight { rect, stroke } if !stroke.is_empty() => {
                stroke.truncate(MAX_POINTS);
                let half = if width.is_finite() { width.clamp(0.0, MAX_WIDTH) / 2.0 } else { 0.0 };
                *rect = Rect::bounding(stroke).inflate(half).finite();
                stroke.clear();
            }
            Shape::Text { text, .. } => truncate_text(text, MAX_TEXT),
            Shape::Callout { text, tails, .. } => {
                truncate_text(text, MAX_TEXT);
                tails.truncate(MAX_TAILS - 1);
            }
            Shape::Step { number, .. } => *number = (*number).min(9_999),
            Shape::Arrow { bend, .. } | Shape::Line { bend, .. } => *bend = clamp_bend(*bend),
            Shape::Polygon { sides, corners, .. } => {
                *sides = (*sides).clamp(MIN_SIDES, MAX_SIDES);
                corners.truncate(MAX_CORNERS);
                for c in corners.iter_mut() {
                    *c = Pt::new(c.x.clamp(0.0, 1.0), c.y.clamp(0.0, 1.0)).finite();
                }
                // Fewer than three corners is no outline: a polygon of `sides` instead.
                if corners.len() < 3 {
                    corners.clear();
                }
            }
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
        let o = Object::new(
            ObjectId(3),
            Shape::Arrow { from: Pt::new(1.0, 2.0), to: Pt::new(3.0, 4.0), heads: ArrowHeads::Both, bend: 0.0, head: HeadShape::Notched },
            style(),
        );
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
        let line = Shape::Line { from: Pt::new(0.0, 0.0), to: Pt::new(100.0, 0.0), bend: 0.0 };
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
        let step = Shape::step(Pt::new(10.0, 10.0), 1);
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
        let mut a = Shape::Arrow { from: Pt::ZERO, to: Pt::new(5.0, 5.0), heads: ArrowHeads::End, bend: 0.0, head: HeadShape::Notched };
        a.drag_handle(&s, Handle::End, Pt::new(9.0, 1.0), false);
        assert_eq!(a, Shape::Arrow { from: Pt::ZERO, to: Pt::new(9.0, 1.0), heads: ArrowHeads::End, bend: 0.0, head: HeadShape::Notched });
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
        t = t.sanitized(0.0);
        assert!(t.text().unwrap().len() <= MAX_TEXT);
        let nan = Shape::Line { from: Pt::new(f32::NAN, 1.0), to: Pt::new(f32::INFINITY, 2.0), bend: 0.0 }.sanitized(0.0);
        assert_eq!(nan, Shape::Line { from: Pt::new(0.0, 1.0), to: Pt::new(0.0, 2.0), bend: 0.0 });
    }

    /// The highlighter draws boxes; a stroke it drew before (an older file, or a script giving
    /// points) is read as the box round the stroke, its width included, and written as a box.
    #[test]
    fn old_highlighter_strokes_read_as_their_boxes() {
        let old = r#"{"id":1,"type":"highlight","points":[{"x":10,"y":50},{"x":200,"y":50}],"style":{"width":22}}"#;
        let o: Object = serde_json::from_str(old).unwrap();
        let shape = o.shape.sanitized(o.style.width);
        assert_eq!(shape, Shape::highlight(Rect::new(-1.0, 39.0, 212.0, 22.0)));
        assert_eq!(shape.rect(), Some(Rect::new(-1.0, 39.0, 212.0, 22.0)));
        let v = serde_json::to_value(&shape).unwrap();
        assert_eq!(v["type"], "highlight");
        assert!(v.get("points").is_none(), "never written: {v}");
        assert_eq!(serde_json::from_value::<Shape>(v).unwrap().sanitized(22.0), shape);
        // Hostile strokes stay finite and bounded.
        let junk = Shape::Highlight { rect: Rect::default(), stroke: vec![Pt::new(f32::NAN, f32::INFINITY), Pt::new(5.0, 5.0)] }.sanitized(f32::NAN);
        let r = junk.rect().unwrap();
        assert!(r.x.is_finite() && r.y.is_finite() && r.w.is_finite() && r.h.is_finite());
        // A box is a box: it's hit inside, and its handles are its corners and sides.
        let b = Shape::highlight(Rect::new(0.0, 0.0, 100.0, 20.0));
        assert!(b.hit(&style(), Pt::new(50.0, 10.0), 0.0));
        assert!(!b.hit(&style(), Pt::new(50.0, 30.0), 2.0));
        assert_eq!(b.handles(&style()).len(), Handle::BOX.len());
        assert_eq!(b.bounds(&Style { width: 40.0, ..style() }), Rect::new(0.0, 0.0, 100.0, 20.0), "no line round it");
    }

    #[test]
    fn callouts_from_before_forms_read_as_rounded_boxes() {
        let old = r#"{"id":1,"type":"callout","rect":{"x":0,"y":0,"w":100,"h":40},"tail":{"x":10,"y":90},"text":"hi"}"#;
        let o: Object = serde_json::from_str(old).unwrap();
        let Shape::Callout { form, .. } = o.shape else { panic!("a callout") };
        assert_eq!(form, CalloutForm::Rounded);
        let arrow = Shape::Callout {
            rect: Rect::new(0.0, 0.0, 100.0, 40.0),
            tail: Pt::new(150.0, 20.0),
            text: String::new(),
            form: CalloutForm::Arrow,
            tails: Vec::new(),
        };
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
        let cloud = Shape::Callout { rect: r, tail: Pt::new(100.0, 50.0), text: String::new(), form: CalloutForm::Cloud, tails: Vec::new() };
        assert!(cloud.bounds(&Style { width: 0.0, ..style() }).w > r.w);
    }

    #[test]
    fn shape_options_set_only_what_they_say() {
        let r = Rect::new(10.0, 10.0, 100.0, 50.0);
        let callout = Shape::Callout { rect: r, tail: Pt::new(0.0, 0.0), text: "hi".into(), form: CalloutForm::Rounded, tails: Vec::new() };
        let o =
            ShapeOptions { form: Some(CalloutForm::Arrow), tail: Some(TailSpot::Right), heads: Some(ArrowHeads::Both), ..ShapeOptions::default() };
        let Shape::Callout { form, tail, text, rect, .. } = o.applied(callout) else { panic!("still a callout") };
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
        let line = Shape::Line { from: Pt::ZERO, to: Pt::new(5.0, 5.0), bend: 0.0 };
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
    fn a_style_keeps_a_font_by_name_and_reads_nonsense_as_the_bundled_one() {
        let s = Style { font: FontName::new("  DejaVu Serif "), ..Style::default() };
        assert_eq!(s.font.as_str(), "DejaVu Serif");
        let v = serde_json::to_value(s).unwrap();
        assert_eq!(v["font"], "DejaVu Serif");
        assert_eq!(serde_json::from_value::<Style>(v).unwrap(), s);
        // The bundled font isn't written, and styles from before read as it.
        assert!(serde_json::to_value(Style::default()).unwrap().get("font").is_none());
        assert!(serde_json::from_str::<Style>(r#"{"fontSize":20}"#).unwrap().font.is_bundled());
        // Not words, too long, or with control characters: kept in bounds, never an error.
        assert!(serde_json::from_str::<Style>(r#"{"font":42}"#).unwrap().font.is_bundled());
        let long = FontName::new(&"é".repeat(100));
        assert!(long.as_str().len() <= MAX_FONT_NAME && long.as_str().chars().all(|c| c == 'é'));
        assert_eq!(FontName::new("A\u{0}\nB\u{7}").as_str(), "AB");
        assert_eq!(format!("{:?}", FontName::new("Mono")), "\"Mono\"");
    }

    #[test]
    fn stamps_are_found_by_their_names_and_words() {
        let found = |q: &str| StampKind::ALL.into_iter().filter(|k| k.matches(q)).collect::<Vec<_>>();
        assert_eq!(found("").len(), StampKind::ALL.len(), "an empty search finds them all");
        assert_eq!(found("smile"), vec![StampKind::Happy]);
        assert!(found("arrow").len() == 4 && found("arrow").iter().all(|k| k.label().starts_with("Arrow")));
        assert_eq!(found("ARROW LE"), vec![StampKind::ArrowLeft], "every word, any case, word starts");
        assert!(found("emoji").iter().all(|k| k.is_face()) && found("emoji").len() == 9);
        assert!(found("teapot").is_empty());
        assert_eq!(found("ok"), vec![StampKind::Check]);
        // Names and colours: each name its own, every colour seen.
        for (i, a) in StampKind::ALL.iter().enumerate() {
            assert!(StampKind::ALL.iter().skip(i + 1).all(|b| a.label() != b.label()), "{a:?}");
            assert!(a.color().a == 255);
        }
        assert!(StampKind::FIRST.iter().all(|k| StampKind::ALL.contains(k)));
        // Old files name the first seven as they always did; new ones are read.
        assert_eq!(serde_json::from_str::<StampKind>(r#""heart""#).unwrap(), StampKind::Heart);
        assert_eq!(serde_json::from_str::<StampKind>(r#""arrowUp""#).unwrap(), StampKind::ArrowUp);
        assert_eq!(serde_json::to_string(&StampKind::NoEntry).unwrap(), r#""noEntry""#);
    }

    #[test]
    fn polygons_and_stars_fill_their_box_and_are_hit_inside_when_filled() {
        let r = Rect::new(10.0, 20.0, 100.0, 80.0);
        for sides in MIN_SIDES..=MAX_SIDES {
            for star in [false, true] {
                let pts = polygon_points(r, sides, star);
                assert_eq!(pts.len(), sides as usize * if star { 2 } else { 1 });
                let b = Rect::bounding(&pts);
                assert!(
                    (b.x - r.x).abs() < 0.01 && (b.w - r.w).abs() < 0.01 && (b.y - r.y).abs() < 0.01 && (b.h - r.h).abs() < 0.01,
                    "{sides} {star} {b:?}"
                );
            }
        }
        // A triangle stands on its base, its point at the top middle.
        let tri = polygon_points(r, 3, false);
        assert!(tri.first().is_some_and(|p| p.distance(Pt::new(60.0, 20.0)) < 0.01), "{tri:?}");
        assert!(tri.iter().skip(1).all(|p| (p.y - 100.0).abs() < 0.01));
        // Nonsense corners are kept in range, and an empty box gives finite points.
        assert_eq!(polygon_points(r, 0, false).len(), 3);
        assert_eq!(polygon_points(r, 99, true).len(), 24);
        assert!(polygon_points(Rect::new(f32::NAN, 0.0, 0.0, 0.0), 5, true).iter().all(|p| p.x.is_finite() && p.y.is_finite()));
        let shape = Shape::Polygon { rect: r, sides: 3, star: false, corners: Vec::new() };
        let s = style();
        assert!(shape.hit(&s, Pt::new(60.0, 21.0), 2.0), "on its point");
        assert!(!shape.hit(&s, Pt::new(60.0, 70.0), 2.0), "inside an unfilled one");
        assert!(shape.hit(&Style { fill: Color::WHITE, ..s }, Pt::new(60.0, 70.0), 2.0), "inside a filled one");
        assert!(!shape.hit(&Style { fill: Color::WHITE, ..s }, Pt::new(14.0, 24.0), 2.0), "a corner of its box is not in it");
        assert_eq!(shape.handles(&s).len(), Handle::BOX.len());
        // From a file: three corners unless it says, kept in range.
        let p: Shape = serde_json::from_str(r#"{"type":"polygon","rect":{"x":0,"y":0,"w":5,"h":5}}"#).unwrap();
        assert_eq!(p, Shape::Polygon { rect: Rect::new(0.0, 0.0, 5.0, 5.0), sides: 3, star: false, corners: Vec::new() });
        let p: Shape = serde_json::from_str(r#"{"type":"polygon","rect":{"x":0,"y":0,"w":5,"h":5},"sides":4000,"star":true}"#).unwrap();
        assert!(matches!(p.sanitized(1.0), Shape::Polygon { sides: 12, star: true, .. }));
    }

    #[test]
    fn shape_options_turn_boxes_into_polygons_and_back() {
        let r = Rect::new(0.0, 0.0, 50.0, 40.0);
        let star = ShapeOptions { sides: Some(5), star: Some(true), ..ShapeOptions::default() };
        assert_eq!(star.applied(Shape::Rect { rect: r, radius: 3.0 }), Shape::Polygon { rect: r, sides: 5, star: true, corners: Vec::new() });
        assert_eq!(star.applied(Shape::Ellipse { rect: r }), Shape::Polygon { rect: r, sides: 5, star: true, corners: Vec::new() });
        let six = ShapeOptions { sides: Some(6), ..ShapeOptions::default() };
        assert_eq!(
            six.applied(Shape::Polygon { rect: r, sides: 5, star: true, corners: Vec::new() }),
            Shape::Polygon { rect: r, sides: 6, star: true, corners: Vec::new() }
        );
        let round = ShapeOptions { round: Some(true), sides: Some(6), ..ShapeOptions::default() };
        assert_eq!(round.applied(Shape::Polygon { rect: r, sides: 5, star: true, corners: Vec::new() }), Shape::Ellipse { rect: r }, "round wins");
        let square = ShapeOptions { round: Some(false), ..ShapeOptions::default() };
        assert_eq!(square.applied(Shape::Polygon { rect: r, sides: 5, star: true, corners: Vec::new() }), Shape::Rect { rect: r, radius: 0.0 });
        // A look that says nothing of corners leaves a rectangle a rectangle.
        assert_eq!(ShapeOptions::default().applied(Shape::Rect { rect: r, radius: 0.0 }), Shape::Rect { rect: r, radius: 0.0 });
        assert_eq!(ShapeOptions::of(&Shape::Polygon { rect: r, sides: 7, star: false, corners: Vec::new() }).sides, Some(7));
        assert_eq!(Kind::Polygon.family(), Kind::Rect);
    }

    #[test]
    fn arrows_bend_by_their_middle_and_keep_their_bend() {
        let s = style();
        let (from, to) = (Pt::new(0.0, 100.0), Pt::new(200.0, 100.0));
        let mut a = Shape::Arrow { from, to, heads: ArrowHeads::End, bend: 0.0, head: HeadShape::Notched };
        let handles = a.handles(&s);
        assert_eq!(handles.last(), Some(&(Handle::Bend, Pt::new(100.0, 100.0))), "the bend handle is in the middle");
        // Dragged up by 50: bowed to the left of the way it points (up, for one pointing right), a quarter of its length.
        a.drag_handle(&s, Handle::Bend, Pt::new(100.0, 50.0), false);
        let Shape::Arrow { bend, .. } = a else { panic!("an arrow") };
        assert!((bend - 0.25).abs() < 1e-5, "{bend}");
        assert!(a.hit(&s, Pt::new(100.0, 50.0), 2.0), "hit along its curve");
        assert!(!a.hit(&s, Pt::new(100.0, 100.0), 2.0), "not where the straight line was");
        assert!(a.bounds(&s).y <= 50.0 && a.extent().y <= 50.5);
        // A handle dragged back near the line straightens it.
        let mut b = a.clone();
        b.drag_handle(&s, Handle::Bend, Pt::new(80.0, 101.0), false);
        assert!(matches!(b, Shape::Arrow { bend, .. } if bend == 0.0));
        // Moving an end keeps the bow's share: the curve keeps its shape.
        a.drag_handle(&s, Handle::End, Pt::new(400.0, 100.0), false);
        assert_eq!(a.handles(&s).last().map(|h| h.1), Some(Pt::new(200.0, 0.0)));
        // Out of reach, or nonsense, is kept in range.
        assert_eq!(bend_through(from, to, Pt::new(100.0, -1e9)), MAX_BEND);
        assert_eq!(clamp_bend(f32::NAN), 0.0);
        assert_eq!(bent_points(from, from, 0.5, 10), vec![from, from], "no length, no curve");
        // A straight arrow is written as before, and an old one reads straight.
        let straight = Shape::Arrow { from, to, heads: ArrowHeads::End, bend: 0.0, head: HeadShape::Notched };
        assert!(serde_json::to_value(&straight).unwrap().get("bend").is_none());
        let bent = serde_json::to_value(&a).unwrap();
        assert_eq!(bent["bend"].as_f64().map(|b| b as f32), Some(0.25));
        assert_eq!(serde_json::from_value::<Shape>(bent).unwrap(), a);
        // Head shapes: notched unless it says, and written only when not.
        assert!(serde_json::to_value(&straight).unwrap().get("head").is_none());
        let open = ShapeOptions { head: Some(HeadShape::Open), ..ShapeOptions::default() }.applied(straight.clone());
        let v = serde_json::to_value(&open).unwrap();
        assert_eq!(v["head"], "open");
        assert_eq!(serde_json::from_value::<Shape>(v).unwrap(), open);
        assert_eq!(ShapeOptions::of(&open).head, Some(HeadShape::Open));
        // A quarter turn keeps the bow on the same side; a flip turns it round.
        let mut turned = a.clone();
        turned.map_points(&|p| Pt::new(500.0 - p.y, p.x));
        assert!(matches!(turned, Shape::Arrow { bend, .. } if bend == 0.25));
        let mut flipped = a.clone();
        flipped.map_points(&|p| Pt::new(500.0 - p.x, p.y));
        assert!(matches!(flipped, Shape::Arrow { bend, .. } if bend == -0.25));
    }

    #[test]
    fn steps_count_in_letters_and_roman_numerals() {
        let all = |l: StepLabels, ns: &[u32]| ns.iter().map(|n| l.text(*n)).collect::<Vec<_>>().join(" ");
        assert_eq!(all(StepLabels::Numbers, &[1, 2, 10, 9999]), "1 2 10 9999");
        assert_eq!(all(StepLabels::Upper, &[1, 2, 26, 27, 28, 52, 53, 702, 703]), "A B Z AA AB AZ BA ZZ AAA");
        assert_eq!(all(StepLabels::Lower, &[1, 3, 27]), "a c aa");
        assert_eq!(all(StepLabels::Roman, &[1, 4, 9, 14, 40, 90, 400, 1994, 3999]), "I IV IX XIV XL XC CD MCMXCIV MMMCMXCIX");
        assert_eq!(all(StepLabels::RomanLower, &[2, 19]), "ii xix");
        // Past what Roman numerals write, and nought, are figures.
        assert_eq!(all(StepLabels::Roman, &[4000, 0]), "4000 0");
        assert_eq!(StepLabels::Upper.text(0), "0");
        let most = StepLabels::Upper.text(u32::MAX);
        assert!(!most.is_empty() && most.len() <= 8 && most.chars().all(|c| c.is_ascii_uppercase()), "{most}");
        // Old files (before labels and badges) read as numbers on circles, and new ones round trip.
        let old: Object = serde_json::from_str(r#"{"id":1,"type":"step","center":{"x":5,"y":6},"number":3}"#).unwrap();
        assert_eq!(old.shape, Shape::step(Pt::new(5.0, 6.0), 3));
        let new = Shape::Step { center: Pt::ZERO, number: 2, labels: StepLabels::RomanLower, badge: BadgeShape::Hexagon };
        let v = serde_json::to_value(&new).unwrap();
        assert_eq!((v["labels"].as_str(), v["badge"].as_str()), (Some("romanLower"), Some("hexagon")));
        assert_eq!(serde_json::from_value::<Shape>(v).unwrap(), new);
        assert_eq!(new.step_label().as_deref(), Some("ii"));
        assert!(serde_json::from_str::<Shape>(r#"{"type":"step","center":{"x":0,"y":0},"number":1,"badge":"teapot"}"#).is_err());
    }

    #[test]
    fn badges_are_hit_and_bounded_by_their_shape() {
        let s = Kind::Step.default_style();
        let r = step_radius(s.font_size);
        let c = Pt::new(100.0, 100.0);
        for badge in BadgeShape::ALL {
            let shape = Shape::Step { center: c, number: 1, labels: StepLabels::Numbers, badge };
            assert!(shape.hit(&s, c, 0.0), "{badge:?} in its middle");
            assert!(!shape.hit(&s, Pt::new(100.0 + r * 3.0, 100.0), 0.0), "{badge:?} far off");
            let b = shape.bounds(&s);
            assert!(b.contains(c) && b.w >= r && b.h >= r, "{badge:?} {b:?}");
        }
        // A diamond's corners are empty; a square's aren't.
        let corner = Pt::new(100.0 + r * 0.85, 100.0 + r * 0.85);
        let diamond = Shape::Step { center: c, number: 1, labels: StepLabels::Numbers, badge: BadgeShape::Diamond };
        let square = Shape::Step { center: c, number: 1, labels: StepLabels::Numbers, badge: BadgeShape::Square };
        assert!(!diamond.hit(&s, corner, 0.0) && square.hit(&s, corner, 0.0));
        // A pin reaches down to its point, and its bounds hold it.
        let pin = Shape::Step { center: c, number: 1, labels: StepLabels::Numbers, badge: BadgeShape::Pin };
        let tip = Pt::new(100.0, 100.0 + r * (PIN_POINT - 0.1));
        assert!(pin.hit(&s, tip, 0.0) && !Shape::step(c, 1).hit(&s, tip, 0.0));
        assert!(pin.bounds(&s).bottom() >= 100.0 + r * PIN_POINT);
        // Options set the labels and the badge and keep the number.
        let o = ShapeOptions { labels: Some(StepLabels::Upper), badge: Some(BadgeShape::Rounded), ..ShapeOptions::default() };
        assert_eq!(o.applied(Shape::step(c, 4)), Shape::Step { center: c, number: 4, labels: StepLabels::Upper, badge: BadgeShape::Rounded });
        assert_eq!(ShapeOptions::of(&o.applied(Shape::step(c, 4))), o);
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
