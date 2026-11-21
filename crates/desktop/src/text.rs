//! Text read off a picture (Grab Text), on this computer and nowhere else.
//!
//! On Windows it is the system's own text recognition (`Windows.Media.Ocr`, part of Windows 10
//! and 11), in the first of the owner's languages it has. Nothing is sent anywhere and nothing
//! is downloaded. Elsewhere there is none yet, and [`read`] says so.
//!
//! A small picture is made larger first (screen text is small for a recogniser); a picture taller
//! than the recogniser takes is read in bands, each line kept from the band it is wholly in.

use screengrabcraft_model::Raster;

/// A line of text and where it is on the picture, in its pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The most lines a picture gives (a page of tiny print is still a page).
pub const MAX_LINES: usize = 5000;

/// The lines read, one to a line, in the order they are read.
pub fn joined(lines: &[Line]) -> String {
    let mut s = String::new();
    for l in lines {
        if !s.is_empty() {
            s.push('\n');
        }
        s.push_str(l.text.trim_end());
    }
    s
}

/// Whether this computer can read text off a picture, or why not.
pub fn ready() -> Result<(), String> {
    imp::ready()
}

/// The text in `picture`, line by line. An error says plainly why there is none (no recogniser
/// here, or none for the owner's languages); a picture with no text gives no lines.
pub fn read(picture: &Raster) -> Result<Vec<Line>, String> {
    let (scale, pic) = enlarged(picture)?;
    let mut lines = imp::read(&pic)?;
    if scale > 1 {
        let f = scale as f32;
        for l in &mut lines {
            l.x /= f;
            l.y /= f;
            l.w /= f;
            l.h /= f;
        }
    }
    lines.truncate(MAX_LINES);
    Ok(lines)
}

/// How many times larger a picture is read: small ones twice (screen text is often under the
/// size a recogniser reads well), others as they are.
fn scale_for(w: u32, h: u32) -> u32 {
    if w.max(h) <= 1600 && u64::from(w) * u64::from(h) <= 1_500_000 { 2 } else { 1 }
}

fn enlarged(picture: &Raster) -> Result<(u32, Raster), String> {
    let (w, h) = (picture.width(), picture.height());
    let scale = scale_for(w, h);
    if scale == 1 {
        return Ok((1, picture.clone()));
    }
    let (sw, sh) = (w.saturating_mul(scale), h.saturating_mul(scale));
    match screengrabcraft_render::effects::resize(picture, sw, sh) {
        Ok(big) => Ok((scale, big)),
        // Too large to enlarge after all: read it as it is.
        Err(_) => Ok((1, picture.clone())),
    }
}

/// The picture's pixels as Windows' recogniser takes them: blue, green, red, alpha, every pixel
/// opaque (see-through parts read as white).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn bgra_on_white(picture: &Raster) -> Vec<u8> {
    let mut out = Vec::with_capacity(picture.pixels().len());
    for &[r, g, b, a] in picture.pixels().as_chunks::<4>().0 {
        let a = u32::from(a);
        let over = |c: u8| ((u32::from(c) * a + 255 * (255 - a) + 127) / 255).min(255) as u8;
        out.extend_from_slice(&[over(b), over(g), over(r), 255]);
    }
    out
}

/// Where each band starts and ends, for a picture `height` tall read `max` at a time with
/// `overlap` shared between neighbours; one band if it fits.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn bands(height: u32, max: u32, overlap: u32) -> Vec<(u32, u32)> {
    let max = max.max(64);
    if height <= max {
        return vec![(0, height)];
    }
    let overlap = overlap.min(max / 2);
    let step = max.saturating_sub(overlap).max(1);
    let mut out = Vec::new();
    let mut top = 0u32;
    while top < height && out.len() < 1000 {
        let bottom = top.saturating_add(max).min(height);
        out.push((top, bottom));
        if bottom == height {
            break;
        }
        top = top.saturating_add(step);
    }
    out
}

/// Whether a line read in the band `i` of `all` is kept there: its middle is in the part of the
/// band no neighbour reads better (each overlap is split down its middle).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn keeps(all: &[(u32, u32)], i: usize, middle: f32) -> bool {
    let Some(&(top, bottom)) = all.get(i) else { return false };
    let from = match i.checked_sub(1).and_then(|p| all.get(p)) {
        Some(&(_, prev_bottom)) => (top as f32 + prev_bottom as f32) / 2.0,
        None => f32::MIN,
    };
    let to = match all.get(i + 1) {
        Some(&(next_top, _)) => (next_top as f32 + bottom as f32) / 2.0,
        None => f32::MAX,
    };
    middle >= from && middle < to
}

#[cfg(target_os = "windows")]
mod imp {
    use screengrabcraft_model::Raster;
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;

    use super::{Line, bands, bgra_on_white, keeps};

    fn err(e: windows::core::Error) -> String {
        e.message().to_string()
    }

    fn engine() -> Result<OcrEngine, String> {
        OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| {
            "Windows has no text recognition for your languages. Add a language with its \"Optical character recognition\" feature in Settings › Time & language › Language & region.".to_string()
        })
    }

    pub fn ready() -> Result<(), String> {
        engine().map(drop)
    }

    pub fn read(picture: &Raster) -> Result<Vec<Line>, String> {
        let engine = engine()?;
        let max = OcrEngine::MaxImageDimension().unwrap_or(2600).clamp(256, 20_000);
        let (w, h) = (picture.width(), picture.height());
        // Wider than it takes: made narrower (rare: a screen is wider than tall).
        let (fit, sx) = if w > max {
            let nh = ((h as f64) * (max as f64) / (w as f64)).round().max(1.0) as u32;
            (screengrabcraft_render::effects::resize(picture, max, nh).map_err(|e| e.to_string())?, w as f32 / max as f32)
        } else {
            (picture.clone(), 1.0)
        };
        let all = bands(fit.height(), max, 160);
        let mut lines = Vec::new();
        for (i, &(top, bottom)) in all.iter().enumerate() {
            let Some(band) = fit.crop(0, top, fit.width(), bottom.saturating_sub(top)) else { continue };
            for l in read_band(&engine, &band)? {
                let middle = top as f32 + l.y + l.h / 2.0;
                if keeps(&all, i, middle) {
                    lines.push(Line { text: l.text, x: l.x * sx, y: (l.y + top as f32) * sx, w: l.w * sx, h: l.h * sx });
                }
                if lines.len() >= super::MAX_LINES {
                    return Ok(lines);
                }
            }
        }
        Ok(lines)
    }

    fn read_band(engine: &OcrEngine, band: &Raster) -> Result<Vec<Line>, String> {
        let (w, h) = (i32::try_from(band.width()).map_err(|e| e.to_string())?, i32::try_from(band.height()).map_err(|e| e.to_string())?);
        let buffer = CryptographicBuffer::CreateFromByteArray(&bgra_on_white(band)).map_err(err)?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, w, h).map_err(err)?;
        let result = engine.RecognizeAsync(&bitmap).map_err(err)?.join().map_err(err)?;
        let mut out = Vec::new();
        for line in result.Lines().map_err(err)? {
            let text = line.Text().map_err(err)?.to_string_lossy();
            if text.trim().is_empty() {
                continue;
            }
            // The line's box: round all of its words.
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for word in line.Words().map_err(err)? {
                if let Ok(r) = word.BoundingRect() {
                    x0 = x0.min(r.X);
                    y0 = y0.min(r.Y);
                    x1 = x1.max(r.X + r.Width);
                    y1 = y1.max(r.Y + r.Height);
                }
            }
            let (x, y, w, h) = if x1 >= x0 && y1 >= y0 { (x0, y0, x1 - x0, y1 - y0) } else { (0.0, 0.0, 0.0, 0.0) };
            out.push(Line { text, x, y, w, h });
        }
        Ok(out)
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use screengrabcraft_model::Raster;

    use super::Line;

    const ONLY_WINDOWS: &str = "Grab Text reads text on Windows only, for now.";

    pub fn ready() -> Result<(), String> {
        Err(ONLY_WINDOWS.to_string())
    }

    pub fn read(_picture: &Raster) -> Result<Vec<Line>, String> {
        Err(ONLY_WINDOWS.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_joined_one_to_a_line() {
        let l = |t: &str| Line { text: t.to_string(), x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        assert_eq!(joined(&[l("Hello world  "), l("Second")]), "Hello world\nSecond");
        assert_eq!(joined(&[]), "");
    }

    #[test]
    fn small_pictures_are_read_larger() {
        assert_eq!(scale_for(800, 600), 2);
        assert_eq!(scale_for(3840, 2160), 1);
        assert_eq!(scale_for(1600, 1200), 1);
    }

    #[test]
    fn see_through_reads_as_white() {
        let pic = Raster::new(2, 1, vec![255, 0, 0, 255, 0, 0, 0, 0]).unwrap();
        assert_eq!(bgra_on_white(&pic), vec![0, 0, 255, 255, 255, 255, 255, 255]);
    }

    #[test]
    fn tall_pictures_are_read_in_overlapping_bands_each_line_kept_once() {
        assert_eq!(bands(500, 1000, 100), vec![(0, 500)]);
        let all = bands(2500, 1000, 100);
        assert_eq!(all, vec![(0, 1000), (900, 1900), (1800, 2500)]);
        // A line in the first overlap (900..1000) belongs to one band only.
        for middle in [10.0, 940.0, 960.0, 1850.0, 2400.0] {
            let n = (0..all.len()).filter(|&i| keeps(&all, i, middle)).count();
            assert_eq!(n, 1, "line at {middle}");
        }
        // Junk never loops for ever.
        assert!(bands(u32::MAX, 0, u32::MAX).len() <= 1000);
    }

    /// Reads made-up text drawn by ScreenGrabCraft's own text renderer, where Windows has a
    /// recogniser for the owner's language (else there is nothing to check).
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_reads_text_drawn_on_a_picture() {
        if ready().is_err() {
            eprintln!("no text recognition for this computer's languages; skipped");
            return;
        }
        let pic = drawn("Invoice 4021 total 19.95");
        let lines = read(&pic).unwrap();
        let text = joined(&lines).to_lowercase();
        assert!(text.contains("invoice"), "read {text:?}");
        assert!(text.contains("4021"), "read {text:?}");
        let first = lines.first().unwrap();
        assert!(first.x >= 0.0 && first.y >= 0.0 && first.x + first.w <= pic.width() as f32 + 2.0, "{first:?}");
    }

    #[cfg(target_os = "windows")]
    fn drawn(text: &str) -> Raster {
        use screengrabcraft_model::{Color, Document, Rect, Shape, Style};
        let base = Raster::filled(420, 60, Color::WHITE).unwrap();
        let mut doc = Document::new(base);
        let style = Style { font_size: 22.0, text_color: Color::BLACK, stroke: Color::TRANSPARENT, ..Style::default() };
        doc.add(Shape::Text { rect: Rect::new(10.0, 12.0, 400.0, 40.0), text: text.to_string() }, style).unwrap();
        screengrabcraft_render::render(&doc).unwrap()
    }
}
