//! Quick styles: ready-made looks for each tool, made from a theme (a set of four colours), and
//! the looks the owner stars as favourites (kept in the editor's settings).

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::object::{ArrowHeads, CalloutForm, Kind, ShapeOptions, StampKind, Style, TailSpot};

/// A look for a tool: the style it draws with, and what it sets about the shape.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickStyle {
    pub kind: Kind,
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub options: ShapeOptions,
}

impl QuickStyle {
    /// Every number in its range, and the kind as the tool it belongs to.
    pub fn clamped(self) -> QuickStyle {
        QuickStyle { kind: self.kind.family(), style: self.style.clamped(), options: self.options.clamped() }
    }

    /// Whether this look describes `current` (a tool's look, or an annotation's): the same tool,
    /// the same style (the text size aside, which a quick style leaves alone), and every option
    /// it sets the same (a tail's spot only where `current` knows it).
    pub fn matches(&self, current: &QuickStyle) -> bool {
        let o = &self.options;
        let c = &current.options;
        let same = |a: Option<bool>, b: Option<bool>| a.is_none() || a == b;
        self.kind.family() == current.kind.family()
            && Style { font_size: current.style.font_size, ..self.style } == current.style
            && (o.heads.is_none() || o.heads == c.heads)
            && (o.radius.is_none() || o.radius == c.radius)
            && (o.form.is_none() || o.form == c.form)
            && (o.tail.is_none() || c.tail.is_none() || o.tail == c.tail)
            && (o.stamp.is_none() || o.stamp == c.stamp)
            && same(o.round, c.round)
    }
}

/// The most favourites kept.
pub const MAX_FAVORITES: usize = 48;

/// A colour of a theme, its name, and the colour of text on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swatch {
    pub name: &'static str,
    pub color: Color,
    pub ink: Color,
}

/// A set of four colours the quick styles are made in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuickTheme {
    pub name: &'static str,
    pub swatches: [Swatch; 4],
}

const DARK_INK: Color = Color::rgb(0x11, 0x11, 0x11);

const fn swatch(name: &'static str, color: Color, ink: Color) -> Swatch {
    Swatch { name, color, ink }
}

/// The themes, the first the one a new editor starts with.
pub static THEMES: [QuickTheme; 4] = [
    QuickTheme {
        name: "Bold",
        swatches: [
            swatch("Red", Color::rgb(0xE5, 0x39, 0x35), Color::WHITE),
            swatch("Green", Color::rgb(0x43, 0xA0, 0x47), Color::WHITE),
            swatch("Blue", Color::rgb(0x1E, 0x88, 0xE5), Color::WHITE),
            swatch("Yellow", Color::rgb(0xFD, 0xD8, 0x35), DARK_INK),
        ],
    },
    QuickTheme {
        name: "Soft",
        swatches: [
            swatch("Coral", Color::rgb(0xF2, 0x8B, 0x82), DARK_INK),
            swatch("Mint", Color::rgb(0x81, 0xC9, 0x95), DARK_INK),
            swatch("Sky", Color::rgb(0x8A, 0xB4, 0xF8), DARK_INK),
            swatch("Butter", Color::rgb(0xFD, 0xE2, 0x93), DARK_INK),
        ],
    },
    QuickTheme {
        name: "Vivid",
        swatches: [
            swatch("Pink", Color::rgb(0xEC, 0x40, 0x7A), Color::WHITE),
            swatch("Orange", Color::rgb(0xFB, 0x8C, 0x00), Color::WHITE),
            swatch("Purple", Color::rgb(0x8E, 0x24, 0xAA), Color::WHITE),
            swatch("Teal", Color::rgb(0x00, 0x89, 0x7B), Color::WHITE),
        ],
    },
    QuickTheme {
        name: "Mono",
        swatches: [
            swatch("Black", DARK_INK, Color::WHITE),
            swatch("Gray", Color::rgb(0x75, 0x75, 0x75), Color::WHITE),
            swatch("Silver", Color::rgb(0xBD, 0xBD, 0xBD), DARK_INK),
            swatch("White", Color::WHITE, DARK_INK),
        ],
    },
];

/// The theme with this name, or the first one.
pub fn theme(name: &str) -> &'static QuickTheme {
    let [first, ..] = &THEMES;
    THEMES.iter().find(|t| t.name.eq_ignore_ascii_case(name)).unwrap_or(first)
}

/// A quick style with the name the panel gives it.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: String,
    pub look: QuickStyle,
}

/// A colour a little darker, for the border round a filled shape.
fn darker(c: Color) -> Color {
    let f = |v: u8| (f32::from(v) * 0.72).round() as u8;
    Color { r: f(c.r), g: f(c.g), b: f(c.b), a: c.a }
}

/// Text that reads on white: the colour itself, or dark ink when it is too light.
fn on_white(s: &Swatch) -> Color {
    if s.color.luminance() > 0.55 { DARK_INK } else { s.color }
}

/// The quick styles of a tool in a theme. Most come in rows, one look in the theme's four
/// colours a row; the effects come in strengths, the stamps one each.
pub fn presets(kind: Kind, theme: &QuickTheme) -> Vec<Preset> {
    let kind = kind.family();
    let base = kind.default_style();
    let none = ShapeOptions::default();
    let mut out = Vec::new();
    let mut row = |variant: &str, make: &dyn Fn(&Swatch) -> (Style, ShapeOptions)| {
        for s in &theme.swatches {
            let (style, options) = make(s);
            out.push(Preset { name: format!("{}, {variant}", s.name), look: QuickStyle { kind, style: style.clamped(), options } });
        }
    };
    let heads = |h: ArrowHeads| ShapeOptions { heads: Some(h), ..none };
    match kind {
        Kind::Arrow => {
            row("arrow", &|s| (Style { stroke: s.color, width: 6.0, shadow: true, dashed: false, ..base }, heads(ArrowHeads::End)));
            row("thick arrow", &|s| (Style { stroke: s.color, width: 12.0, shadow: true, dashed: false, ..base }, heads(ArrowHeads::End)));
            row("two heads", &|s| (Style { stroke: s.color, width: 6.0, shadow: true, dashed: false, ..base }, heads(ArrowHeads::Both)));
            row("dashed arrow", &|s| (Style { stroke: s.color, width: 5.0, shadow: false, dashed: true, ..base }, heads(ArrowHeads::End)));
        }
        Kind::Line => {
            row("line", &|s| (Style { stroke: s.color, width: 4.0, dashed: false, ..base }, none));
            row("thick line", &|s| (Style { stroke: s.color, width: 9.0, dashed: false, ..base }, none));
            row("dashed line", &|s| (Style { stroke: s.color, width: 4.0, dashed: true, ..base }, none));
        }
        Kind::Pen => {
            row("pen", &|s| (Style { stroke: s.color, width: 4.0, ..base }, none));
            row("marker", &|s| (Style { stroke: s.color, width: 10.0, ..base }, none));
        }
        Kind::Highlight => {
            row("highlighter", &|s| (Style { stroke: s.color.with_alpha(120), width: 22.0, ..base }, none));
            row("wide highlighter", &|s| (Style { stroke: s.color.with_alpha(120), width: 40.0, ..base }, none));
        }
        Kind::Rect => {
            row("outline", &|s| (Style { stroke: s.color, fill: Color::TRANSPARENT, width: 4.0, shadow: true, ..base }, none));
            row("tinted", &|s| (Style { stroke: s.color, fill: s.color.with_alpha(64), width: 3.0, shadow: false, ..base }, none));
            row("solid", &|s| (Style { stroke: darker(s.color), fill: s.color, width: 2.0, shadow: true, ..base }, none));
        }
        Kind::Text => {
            let plain = Style { stroke: Color::TRANSPARENT, fill: Color::TRANSPARENT, width: 0.0, bold: true, shadow: false, ..base };
            row("text", &|s| (Style { text_color: s.color, ..plain }, none));
            row("label", &|s| (Style { fill: s.color, text_color: s.ink, ..plain }, none));
            row("boxed text", &|s| (Style { fill: Color::WHITE, stroke: s.color, width: 3.0, text_color: on_white(s), bold: false, ..plain }, none));
        }
        Kind::Callout => {
            let filled =
                |s: &Swatch| Style { fill: s.color, stroke: darker(s.color), width: 2.0, text_color: s.ink, bold: true, shadow: true, ..base };
            let callout = |form: CalloutForm, tail: TailSpot| ShapeOptions { form: Some(form), tail: Some(tail), ..none };
            row("tail down left", &|s| (filled(s), callout(CalloutForm::Rounded, TailSpot::DownLeft)));
            row("tail down right", &|s| (filled(s), callout(CalloutForm::Rounded, TailSpot::DownRight)));
            row("arrow", &|s| (filled(s), callout(CalloutForm::Arrow, TailSpot::Right)));
            row("outlined", &|s| {
                let style = Style { fill: Color::WHITE, stroke: s.color, width: 3.0, text_color: DARK_INK, bold: false, shadow: true, ..base };
                (style, callout(CalloutForm::Rounded, TailSpot::DownLeft))
            });
        }
        Kind::Step => {
            row("badge", &|s| (Style { fill: s.color, text_color: s.ink, stroke: Color::WHITE, width: 2.0, ..base }, none));
            row("ring", &|s| (Style { fill: Color::WHITE, text_color: on_white(s), stroke: s.color, width: 3.0, ..base }, none));
        }
        Kind::Stamp => {
            drop(row);
            for k in StampKind::ALL {
                let look = QuickStyle { kind, style: Style { fill: k.color(), ..base }, options: ShapeOptions { stamp: Some(k), ..none } };
                out.push(Preset { name: k.label().to_string(), look });
            }
            // The stamp in use, in the theme's colours.
            for s in &theme.swatches {
                out.push(Preset { name: s.name.to_string(), look: QuickStyle { kind, style: Style { fill: s.color, ..base }, options: none } });
            }
        }
        Kind::Blur | Kind::Pixelate | Kind::Spotlight | Kind::Magnify => {
            drop(row);
            let strengths: &[(&str, f32)] = match kind {
                Kind::Blur => &[("Light blur", 6.0), ("Medium blur", 14.0), ("Strong blur", 30.0)],
                Kind::Pixelate => &[("Small squares", 6.0), ("Squares", 12.0), ("Large squares", 24.0)],
                Kind::Spotlight => &[("Dim", 0.4), ("Dark", 0.6), ("Darker", 0.82)],
                _ => &[("1.5 times", 1.5), ("2 times", 2.0), ("3 times", 3.0)],
            };
            let rounds: &[(&str, Option<bool>)] = if kind == Kind::Spotlight { &[("", Some(false)), (", round", Some(true))] } else { &[("", None)] };
            for (suffix, round) in rounds {
                for (name, strength) in strengths {
                    let look =
                        QuickStyle { kind, style: Style { strength: *strength, ..base }.clamped(), options: ShapeOptions { round: *round, ..none } };
                    out.push(Preset { name: format!("{name}{suffix}"), look });
                }
            }
        }
        Kind::Ellipse | Kind::Image => drop(row),
    }
    out
}

/// The colours the Fill tool offers in a theme: the theme's four, white, black and none (which
/// makes an area see-through).
pub fn fill_colors(theme: &QuickTheme) -> Vec<(String, Color)> {
    let mut out: Vec<(String, Color)> = theme.swatches.iter().map(|s| (s.name.to_string(), s.color)).collect();
    for (name, c) in [("White", Color::WHITE), ("Black", Color::BLACK), ("None", Color::TRANSPARENT)] {
        if !out.iter().any(|(_, o)| *o == c) {
            out.push((name.to_string(), c));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_drawing_tool_has_quick_styles_in_every_theme() {
        for t in &THEMES {
            for k in Kind::ALL {
                let p = presets(k, t);
                match k {
                    Kind::Image => assert!(p.is_empty()),
                    _ => assert!(p.len() >= 3, "{k:?} in {} has {}", t.name, p.len()),
                }
                for preset in &p {
                    assert_eq!(preset.look.kind, k.family());
                    assert_eq!(preset.look, preset.look.clamped(), "{} is in range", preset.name);
                    assert!(preset.look.matches(&preset.look));
                }
                // Each look is different and so is each name.
                for (i, a) in p.iter().enumerate() {
                    for b in p.iter().skip(i + 1) {
                        assert_ne!(a.look, b.look, "{k:?}: {} and {}", a.name, b.name);
                        assert_ne!(a.name, b.name);
                    }
                }
            }
        }
    }

    #[test]
    fn callouts_come_with_tails_both_ways_and_as_arrows() {
        let p = presets(Kind::Callout, theme("Bold"));
        assert_eq!(p.len(), 16);
        let has = |form: CalloutForm, tail: TailSpot| p.iter().any(|q| q.look.options.form == Some(form) && q.look.options.tail == Some(tail));
        assert!(has(CalloutForm::Rounded, TailSpot::DownLeft));
        assert!(has(CalloutForm::Rounded, TailSpot::DownRight));
        assert!(has(CalloutForm::Arrow, TailSpot::Right));
        // The first row is the theme's colours with text that reads on them.
        let names: Vec<&str> = p.iter().take(4).map(|q| q.name.as_str()).collect();
        assert_eq!(names, ["Red, tail down left", "Green, tail down left", "Blue, tail down left", "Yellow, tail down left"]);
        let yellow = p.get(3).map(|q| q.look.style).unwrap();
        assert!(yellow.text_color.luminance() < 0.1, "dark text on yellow");
    }

    #[test]
    fn a_look_matches_what_it_was_applied_to_whatever_the_text_size() {
        let p = presets(Kind::Callout, theme("Bold"));
        let look = p.first().map(|q| q.look).unwrap();
        let mut current = look;
        current.style.font_size = 60.0;
        current.options.tail = None;
        assert!(look.matches(&current), "the text size and an unknown tail don't count");
        current.style.fill = Color::BLACK;
        assert!(!look.matches(&current));
        let shape = presets(Kind::Ellipse, theme("Bold"));
        let ellipse = QuickStyle { kind: Kind::Ellipse, ..shape.first().map(|q| q.look).unwrap() };
        assert!(shape.first().unwrap().look.matches(&ellipse), "rectangles and ellipses share their looks");
    }

    #[test]
    fn unknown_themes_are_the_first_and_fill_colours_end_with_none() {
        assert_eq!(theme("teapot").name, "Bold");
        assert_eq!(theme("mono").name, "Mono");
        let fills = fill_colors(theme("Mono"));
        assert_eq!(fills.last().map(|(_, c)| *c), Some(Color::TRANSPARENT));
        assert_eq!(fills.iter().filter(|(_, c)| *c == Color::WHITE).count(), 1, "white only once");
        let json = r#"{"kind":"ellipse","style":{"width":1e9},"options":{"radius":-4}}"#;
        let q: QuickStyle = serde_json::from_str::<QuickStyle>(json).unwrap().clamped();
        assert_eq!((q.kind, q.options.radius), (Kind::Rect, Some(0.0)));
        assert!(q.style.width <= crate::object::MAX_WIDTH);
    }
}
