//! QR codes: the words in those a picture shows ([`read`]), and a picture of one made from words
//! ([`make`]). Both on this computer (rqrr reads, qrcode makes): nothing found is opened or sent
//! anywhere, a link read is only words.

use screengrabcraft_model::{Raster, Rect};

/// The most codes read from one picture.
pub const MAX_CODES: usize = 32;
/// The most characters a code is made from (a QR code holds at most about 2,900 bytes).
pub const MAX_TEXT: usize = 1200;
/// The longest side read: a larger picture is read at this size (codes stay large enough).
const MAX_READ_SIDE: u32 = 4096;

/// A QR code found in a picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// What it says.
    pub text: String,
    /// Where it is on the picture (the box round its corners).
    pub rect: Rect,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QrError {
    #[error("there are no words to make a QR code from")]
    Empty,
    #[error("that's too much for a QR code (at most {MAX_TEXT} characters)")]
    TooLong,
    #[error("a QR code couldn't be made from that: {0}")]
    Failed(String),
}

/// The QR codes the picture shows, each once, top to bottom. A part of a code that can't be read
/// is left out; a picture that upsets the reader is read as having none.
pub fn read(pic: &Raster) -> Vec<Found> {
    let (w, h) = (pic.width(), pic.height());
    if w < 21 || h < 21 {
        return Vec::new();
    }
    // Large pictures are read smaller (every `step`th pixel), and where they were on it is scaled back.
    let step = w.max(h).div_ceil(MAX_READ_SIDE).max(1);
    let (rw, rh) = ((w / step) as usize, (h / step) as usize);
    let px = pic.pixels();
    let luma = |x: usize, y: usize| -> u8 {
        let i = ((y * step as usize) * w as usize + x * step as usize) * 4;
        match px.get(i..i + 4) {
            // On white where it is see-through.
            Some([r, g, b, a]) => {
                let l = (u32::from(*r) * 299 + u32::from(*g) * 587 + u32::from(*b) * 114) / 1000;
                let a = u32::from(*a);
                ((l * a + 255 * (255 - a)) / 255) as u8
            }
            _ => 255,
        }
    };
    // (A reader that trips over a picture is no reason to stop the program.)
    let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut img = rqrr::PreparedImage::prepare_from_greyscale(rw, rh, luma);
        let mut out: Vec<Found> = Vec::new();
        for grid in img.detect_grids().into_iter().take(MAX_CODES * 2) {
            let Ok((_, text)) = grid.decode() else { continue };
            let xs = grid.bounds.iter().map(|p| p.x);
            let ys = grid.bounds.iter().map(|p| p.y);
            let (x0, x1) = (xs.clone().min().unwrap_or(0), xs.max().unwrap_or(0));
            let (y0, y1) = (ys.clone().min().unwrap_or(0), ys.max().unwrap_or(0));
            let s = step as f32;
            let rect = Rect::new(x0 as f32 * s, y0 as f32 * s, (x1 - x0).max(1) as f32 * s, (y1 - y0).max(1) as f32 * s);
            if !out.iter().any(|f| f.text == text) && out.len() < MAX_CODES {
                out.push(Found { text, rect });
            }
        }
        out
    }));
    let mut found = found.unwrap_or_default();
    found.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y).then(a.rect.x.total_cmp(&b.rect.x)));
    found
}

/// A QR code of `text`, black on white with its margin of four squares, about `side` pixels
/// across: each square a whole number of pixels (1 to 32), so it stays sharp.
pub fn make(text: &str, side: u32) -> Result<Raster, QrError> {
    if text.trim().is_empty() {
        return Err(QrError::Empty);
    }
    if text.chars().count() > MAX_TEXT {
        return Err(QrError::TooLong);
    }
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M).map_err(|e| QrError::Failed(e.to_string()))?;
    let n = code.width();
    let dark = code.to_colors();
    let scale = (side as usize / (n + 8)).clamp(1, 32);
    let side = (n + 8) * scale;
    let mut px = vec![255u8; side * side * 4];
    for (i, c) in dark.iter().enumerate() {
        if *c != qrcode::Color::Dark {
            continue;
        }
        let (mx, my) = (i % n + 4, i / n + 4);
        for y in my * scale..(my + 1) * scale {
            let row = y * side;
            if let Some(span) = px.get_mut((row + mx * scale) * 4..(row + (mx + 1) * scale) * 4) {
                for p in span.as_chunks_mut::<4>().0 {
                    *p = [0, 0, 0, 255];
                }
            }
        }
    }
    Raster::new(side as u32, side as u32, px).map_err(|e| QrError::Failed(e.to_string()))
}

/// Whether words read from a code are a web address (shown as words, never opened by itself).
pub fn is_link(text: &str) -> bool {
    let t = text.trim().to_ascii_lowercase();
    t.starts_with("http://") || t.starts_with("https://") || t.starts_with("www.")
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::Color;

    use super::*;

    /// `pic` drawn onto a larger white picture at (x, y).
    fn on_white(pic: &Raster, w: u32, h: u32, x: u32, y: u32) -> Raster {
        let mut out = Raster::filled(w, h, Color::WHITE).unwrap();
        let (ow, pw) = (w as usize, pic.width() as usize);
        for (row, src) in pic.pixels().chunks_exact(pw * 4).enumerate() {
            let i = ((y as usize + row) * ow + x as usize) * 4;
            out.pixels_mut()[i..i + src.len()].copy_from_slice(src);
        }
        out
    }

    #[test]
    fn a_code_made_is_read_back_where_it_is() {
        let code = make("https://example.com/order/4021", 150).unwrap();
        let side = code.width() as f32;
        assert_eq!(code.width(), code.height());
        assert!((120.0..=150.0).contains(&side), "{side}");
        let pic = on_white(&code, 400, 300, 120, 40);
        let found = read(&pic);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, "https://example.com/order/4021");
        // Inside its margin of four squares (about a tenth of it each side).
        let r = found[0].rect;
        assert!(r.x > 125.0 && r.x < 120.0 + side * 0.2 && r.y > 45.0 && r.y < 40.0 + side * 0.2, "{r:?}");
        assert!(r.w > side * 0.6 && r.w < side && r.h > side * 0.6, "{r:?}");
        assert!(is_link(&found[0].text) && !is_link("Order 4021"));
    }

    #[test]
    fn two_codes_are_both_read_top_first_and_words_of_any_language_survive() {
        let a = make("Zweite Zeile: Grüße", 100).unwrap();
        let b = make("first", 90).unwrap();
        let mut pic = on_white(&a, 500, 400, 260, 220);
        let top = on_white(&b, b.width(), b.height(), 0, 0);
        for (row, src) in top.pixels().chunks_exact(b.width() as usize * 4).enumerate() {
            let i = ((20 + row) * 500 + 30) * 4;
            pic.pixels_mut()[i..i + src.len()].copy_from_slice(src);
        }
        let texts: Vec<String> = read(&pic).into_iter().map(|f| f.text).collect();
        assert_eq!(texts, vec!["first".to_string(), "Zweite Zeile: Grüße".to_string()]);
    }

    #[test]
    fn pictures_without_codes_and_odd_pictures_read_as_none() {
        assert!(read(&Raster::filled(300, 200, Color::WHITE).unwrap()).is_empty());
        assert!(read(&Raster::filled(5, 5, Color::BLACK).unwrap()).is_empty());
        // Noise.
        let noise: Vec<u8> = (0..200u32 * 200 * 4).map(|i| ((i.wrapping_mul(2_654_435_761) >> 13) & 0xFF) as u8).collect();
        let _ = read(&Raster::new(200, 200, noise).unwrap());
        // See-through: read as on white.
        let code = make("clear", 120).unwrap();
        let mut px = code.pixels().to_vec();
        for p in px.as_chunks_mut::<4>().0 {
            if p[0] == 255 {
                p[3] = 0;
            }
        }
        let clear = Raster::new(code.width(), code.height(), px).unwrap();
        assert_eq!(read(&clear).first().map(|f| f.text.as_str()), Some("clear"));
    }

    #[test]
    fn making_a_code_refuses_nothing_and_too_much() {
        assert_eq!(make("   ", 4), Err(QrError::Empty));
        assert_eq!(make(&"x".repeat(MAX_TEXT + 1), 4), Err(QrError::TooLong));
        let big = make(&"x".repeat(MAX_TEXT), 0).unwrap();
        assert!(big.width() < 200);
        // A whole number of pixels a square: 29 squares (21 and the margin) in 99 px are 3 each.
        assert_eq!(make("a", 99).unwrap().width(), 29 * 3);
        assert_eq!(make("a", u32::MAX).unwrap().width(), 29 * 32);
    }
}
