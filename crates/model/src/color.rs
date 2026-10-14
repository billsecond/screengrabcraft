//! Colours: 8-bit RGBA, written `#RRGGBB` or `#RRGGBBAA` in files and parameters.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A colour with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    /// The default ink of arrows and shapes: a strong red that reads on light and dark pictures.
    pub const RED: Color = Color::rgb(0xE5, 0x39, 0x35);
    pub const YELLOW: Color = Color::rgb(0xFF, 0xD6, 0x00);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    pub fn is_transparent(self) -> bool {
        self.a == 0
    }

    /// The same colour with its alpha multiplied by `f` (0 to 1).
    pub fn faded(self, f: f32) -> Color {
        let f = if f.is_finite() { f.clamp(0.0, 1.0) } else { 1.0 };
        Color { a: (f32::from(self.a) * f).round() as u8, ..self }
    }

    pub fn with_alpha(self, a: u8) -> Color {
        Color { a, ..self }
    }

    /// Relative luminance (0 dark to 1 light), for choosing a readable text colour on it.
    pub fn luminance(self) -> f32 {
        let lin = |c: u8| {
            let c = f32::from(c) / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// Black or white, whichever reads better on this colour.
    pub fn contrasting(self) -> Color {
        if self.luminance() > 0.4 { Color::BLACK } else { Color::WHITE }
    }

    /// `#RRGGBB`, or `#RRGGBBAA` when not opaque.
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    /// Reads `#RGB`, `#RRGGBB` or `#RRGGBBAA` (the `#` is optional).
    pub fn from_hex(text: &str) -> Option<Color> {
        let hex = text.trim().trim_start_matches('#');
        if !hex.is_ascii() {
            return None;
        }
        let byte = |i: usize| hex.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok());
        match hex.len() {
            3 => {
                let nib = |i: usize| hex.get(i..=i).and_then(|s| u8::from_str_radix(s, 16).ok()).map(|v| v * 17);
                Some(Color::rgb(nib(0)?, nib(1)?, nib(2)?))
            }
            6 => Some(Color::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Color, D::Error> {
        let text = String::deserialize(d)?;
        Color::from_hex(&text).ok_or_else(|| serde::de::Error::custom(format!("not a colour: {text:?} (write #RRGGBB or #RRGGBBAA)")))
    }
}

/// The colours offered first in the colour menus: strong enough to read on screenshots, and
/// telling apart for people who see red and green alike (red, orange, yellow, green, blue,
/// purple, plus black, grey and white).
pub const PALETTE: [Color; 10] = [
    Color::rgb(0xE5, 0x39, 0x35),
    Color::rgb(0xFB, 0x8C, 0x00),
    Color::rgb(0xFF, 0xD6, 0x00),
    Color::rgb(0x2E, 0x9E, 0x4F),
    Color::rgb(0x1E, 0x88, 0xE5),
    Color::rgb(0x8E, 0x24, 0xAA),
    Color::rgb(0x11, 0x11, 0x11),
    Color::rgb(0x75, 0x75, 0x75),
    Color::rgb(0xFF, 0xFF, 0xFF),
    Color::rgb(0xEC, 0x40, 0x7A),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        assert_eq!(Color::from_hex("#E53935"), Some(Color::RED));
        assert_eq!(Color::from_hex("e53935"), Some(Color::RED));
        assert_eq!(Color::from_hex("#fff"), Some(Color::WHITE));
        assert_eq!(Color::from_hex("#11223344"), Some(Color::rgba(0x11, 0x22, 0x33, 0x44)));
        assert_eq!(Color::RED.to_hex(), "#E53935");
        assert_eq!(Color::rgba(1, 2, 3, 4).to_hex(), "#01020304");
        for bad in ["", "#", "#12345", "#zzzzzz", "#ééé", "#1234567890"] {
            assert_eq!(Color::from_hex(bad), None, "{bad}");
        }
    }

    #[test]
    fn serde_is_a_hex_string() {
        let json = serde_json::to_string(&Color::RED).unwrap();
        assert_eq!(json, "\"#E53935\"");
        assert_eq!(serde_json::from_str::<Color>(&json).unwrap(), Color::RED);
        assert!(serde_json::from_str::<Color>("\"red\"").is_err());
        assert!(serde_json::from_str::<Color>("12").is_err());
    }

    #[test]
    fn contrast_and_fade() {
        assert_eq!(Color::YELLOW.contrasting(), Color::BLACK);
        assert_eq!(Color::rgb(0x1E, 0x88, 0xE5).contrasting(), Color::WHITE);
        assert_eq!(Color::RED.faded(0.5).a, 128);
        assert_eq!(Color::RED.faded(f32::NAN).a, 255);
    }
}
