//! Edit text in the picture (`image.editText`): a line of text read off the picture (Grab Text's
//! reader) is covered with the colour round it, and an editable text annotation is put in its
//! place in the same size and colour, its letters where the old ones were.

use screengrabcraft_model::object::{MAX_FONT, MIN_FONT};
use screengrabcraft_model::{Color, FontName, Kind, Pt, Raster, Rect, Shape, Style, TextLine};
use serde_json::{Value, json};

use crate::registry::{boolean, color, num};
use crate::session::Session;
use crate::{EngineError, Result};

/// The colours of a line of text on `picture` in `r`: the background's (the commonest colour of
/// the ring just round it) and the letters' (the colours inside farthest from it).
pub fn line_colours(picture: &Raster, r: Rect) -> Option<(Color, Color)> {
    let (x, y, w, h) = r.pixel_bounds(picture.width(), picture.height())?;
    // The ring: the pixels just outside the box (within the picture).
    let ring = r.inflate(2.0).pixel_bounds(picture.width(), picture.height())?;
    let mut counts: std::collections::HashMap<[u8; 3], (u32, [u64; 4])> = std::collections::HashMap::new();
    let inside = |px: u32, py: u32| px >= x && py >= y && px < x + w && py < y + h;
    for py in ring.1..ring.1 + ring.3 {
        for px in ring.0..ring.0 + ring.2 {
            if inside(px, py) {
                continue;
            }
            let Some(c) = picture.pixel(px, py) else { continue };
            let e = counts.entry([c.r >> 4, c.g >> 4, c.b >> 4]).or_insert((0, [0; 4]));
            e.0 += 1;
            for (s, v) in e.1.iter_mut().zip([c.r, c.g, c.b, c.a]) {
                *s += u64::from(v);
            }
        }
    }
    // A line with nothing round it (all of the picture): its own commonest colour.
    if counts.is_empty() {
        for py in y..y + h {
            for px in x..x + w {
                let Some(c) = picture.pixel(px, py) else { continue };
                let e = counts.entry([c.r >> 4, c.g >> 4, c.b >> 4]).or_insert((0, [0; 4]));
                e.0 += 1;
                for (s, v) in e.1.iter_mut().zip([c.r, c.g, c.b, c.a]) {
                    *s += u64::from(v);
                }
            }
        }
    }
    let (_, (n, sum)) = counts.into_iter().max_by_key(|(k, (n, _))| (*n, *k))?;
    let n = u64::from(n.max(1));
    let avg = |k: usize| (sum.get(k).copied().unwrap_or(0) / n).min(255) as u8;
    let bg = Color::rgba(avg(0), avg(1), avg(2), avg(3));
    let dist = |c: Color| {
        let d = |a: u8, b: u8| f32::from(a) - f32::from(b);
        (d(c.r, bg.r).powi(2) + d(c.g, bg.g).powi(2) + d(c.b, bg.b).powi(2)).sqrt()
    };
    let mut far = 0f32;
    for py in y..y + h {
        for px in x..x + w {
            if let Some(c) = picture.pixel(px, py) {
                far = far.max(dist(c));
            }
        }
    }
    let lum = 0.299 * f32::from(bg.r) + 0.587 * f32::from(bg.g) + 0.114 * f32::from(bg.b);
    if far < 40.0 {
        // No letters to see: dark on light, light on dark.
        return Some((bg, if lum > 128.0 { Color::rgb(0x20, 0x20, 0x20) } else { Color::WHITE }));
    }
    let (mut k, mut s) = (0u64, [0u64; 3]);
    for py in y..y + h {
        for px in x..x + w {
            if let Some(c) = picture.pixel(px, py)
                && dist(c) >= far * 0.7
            {
                k += 1;
                for (t, v) in s.iter_mut().zip([c.r, c.g, c.b]) {
                    *t += u64::from(v);
                }
            }
        }
    }
    let k = k.max(1);
    let ink = Color::rgb((s[0] / k).min(255) as u8, (s[1] / k).min(255) as u8, (s[2] / k).min(255) as u8);
    Some((Color { a: 255, ..bg }, ink))
}

/// Where the letters of `text` in `style` are drawn, from the top left of where it is laid out:
/// (left, top, width, height) of their ink. None when nothing is drawn.
fn ink_box(text: &str, style: &Style) -> Option<Rect> {
    let lay = screengrabcraft_render::text::layout(text, style, None);
    let path = screengrabcraft_render::text::text_path(&lay, style, Pt::new(0.0, 0.0), lay.width)?;
    let b = path.bounds();
    (b.width() > 0.0 && b.height() > 0.0).then(|| Rect::new(b.left(), b.top(), b.width(), b.height()))
}

/// The font size whose letters of `text` are as tall as `r` (and no wider than it, near enough).
pub fn matching_size(text: &str, r: Rect, style: &Style) -> f32 {
    let probe = Style { font_size: 100.0, ..*style };
    let Some(ink) = ink_box(text, &probe) else { return (r.h * 1.3).clamp(MIN_FONT, MAX_FONT) };
    let by_height = 100.0 * r.h / ink.h;
    let by_width = 100.0 * r.w / ink.w;
    // Another font is wider or narrower: kept as tall as the line, unless that would run well
    // past its end.
    let size = if by_width < by_height { by_width.max(by_height * 0.85) } else { by_height };
    if size.is_finite() { size.clamp(MIN_FONT, MAX_FONT) } else { (r.h * 1.3).clamp(MIN_FONT, MAX_FONT) }
}

pub(crate) fn edit_text(s: &mut Session, p: &Value) -> Result<Value> {
    let line: TextLine = p
        .get("line")
        .and_then(|v| serde_json::from_value::<TextLine>(v.clone()).ok())
        .map(TextLine::sanitized)
        .ok_or_else(|| EngineError::Params("`line` is a line read off the picture: {\"text\", \"rect\"}".to_string()))?;
    let r = line.rect.finite();
    let picture = s.doc.bounds();
    let r = r
        .intersect(picture)
        .filter(|r| r.w >= 1.0 && r.h >= 1.0)
        .ok_or_else(|| EngineError::Params("the line is outside the picture".to_string()))?;
    let text: String = match p.get("text") {
        None | Some(Value::Null) => line.text.trim().to_string(),
        Some(Value::String(t)) => t.chars().take(screengrabcraft_model::object::MAX_TEXT / 4).collect(),
        Some(_) => return Err(EngineError::Params("`text` is the words to put there".to_string())),
    };
    let (found_bg, found_ink) = line_colours(&s.doc.base, r).unwrap_or((Color::WHITE, Color::BLACK));
    let background = color(p, "background", found_bg)?;
    let ink = color(p, "color", found_ink)?;
    let font = match p.get("font") {
        Some(Value::String(f)) => FontName::new(f),
        _ => FontName::BUNDLED,
    };
    let bold = boolean(p, "bold", false);
    let mut style = Style {
        text_color: ink,
        stroke: Color::TRANSPARENT,
        fill: Color::TRANSPARENT,
        shadow: false,
        bold,
        font,
        text_outline: Color::TRANSPARENT,
        ..Kind::Text.default_style()
    };
    style.font_size = match num(p, "fontSize") {
        Some(f) => f.clamp(MIN_FONT, MAX_FONT),
        None => matching_size(if text.trim().is_empty() { &line.text } else { &text }, r, &style),
    };
    // The cover: the line's box, a little larger, in the colour round it.
    let pad = (r.h * 0.15).clamp(1.0, 6.0);
    let cover_style = Style { stroke: Color::TRANSPARENT, fill: background, width: 0.0, shadow: false, ..Kind::Rect.default_style() };
    let cover = s.doc.add(Shape::Rect { rect: r.inflate(pad).intersect(picture).unwrap_or(r), radius: 0.0 }, cover_style)?;
    // The text, its letters' top left where the old ones' were, with room to type a little more
    // on the line before it wraps.
    let tpad = screengrabcraft_render::text::padding(&style);
    let ink_at = ink_box(&text, &style).unwrap_or(Rect::new(0.0, 0.0, r.w, r.h));
    let lay = screengrabcraft_render::text::layout(&text, &style, None);
    let rect = Rect::new(r.x - tpad - ink_at.x, r.y - tpad - ink_at.y, lay.width + 2.0 * tpad + style.font_size * 3.0, lay.height + 2.0 * tpad);
    let id = s.doc.add(Shape::Text { rect, text }, style)?;
    s.selection = vec![id];
    Ok(json!({
        "cover": cover,
        "text": id,
        "fontSize": style.font_size,
        "color": ink.to_hex(),
        "background": background.to_hex(),
    }))
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::{Document, ObjectId};

    use super::*;

    /// "Total 19.95" in dark blue on a pale yellow picture, drawn by the text renderer.
    fn session() -> (Session, Rect) {
        let base = Raster::filled(320, 80, Color::rgb(255, 248, 220)).unwrap();
        let mut doc = Document::new(base);
        let style = Style { font_size: 24.0, text_color: Color::rgb(20, 40, 120), stroke: Color::TRANSPARENT, ..Style::default() };
        doc.add(Shape::Text { rect: Rect::new(20.0, 20.0, 260.0, 40.0), text: "Total 19.95".to_string() }, style).unwrap();
        let shown = screengrabcraft_render::render(&doc).unwrap();
        // Where the letters are: the box round the pixels that aren't the background.
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..shown.height() {
            for x in 0..shown.width() {
                if shown.pixel(x, y) != Some(Color::rgb(255, 248, 220)) {
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                }
            }
        }
        let line = Rect::new(x0 as f32, y0 as f32, (x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
        (Session::new(Document::new(shown)), line)
    }

    #[test]
    fn colours_round_and_in_a_line_are_found() {
        let (s, line) = session();
        let (bg, ink) = line_colours(&s.doc.base, line).unwrap();
        assert_eq!(bg, Color::rgb(255, 248, 220));
        let near = |a: u8, b: u8| a.abs_diff(b) <= 40;
        assert!(near(ink.r, 20) && near(ink.g, 40) && near(ink.b, 120), "{ink:?}");
        // A plain area has no letters: dark on light.
        assert_eq!(line_colours(&s.doc.base, Rect::new(0.0, 0.0, 10.0, 10.0)).unwrap().1, Color::rgb(0x20, 0x20, 0x20));
    }

    #[test]
    fn the_line_is_covered_and_a_text_put_in_its_place_in_its_size_and_colour() {
        let (mut s, line) = session();
        let out = s.execute("image.editText", &json!({"line": {"text": "Total 19.95", "rect": line}, "text": "Total 24.50"})).unwrap();
        let size = out["fontSize"].as_f64().unwrap() as f32;
        assert!((size - 24.0).abs() < 2.5, "{size}");
        let cover = s.doc.get(ObjectId(out["cover"].as_u64().unwrap())).unwrap();
        assert_eq!(cover.style.fill, Color::rgb(255, 248, 220));
        assert!(cover.shape.rect().unwrap().encloses(line));
        let text = s.doc.get(ObjectId(out["text"].as_u64().unwrap())).unwrap();
        assert_eq!(text.shape.text(), Some("Total 24.50"));
        assert_eq!(s.selected(), vec![text.id]);
        // Drawn, the new letters sit where the old ones were (within a few pixels).
        let shown = screengrabcraft_render::render(&s.doc).unwrap();
        let (mut x0, mut y0) = (u32::MAX, u32::MAX);
        for y in 0..shown.height() {
            for x in 0..shown.width() {
                if shown.pixel(x, y).is_some_and(|c| c.r < 150) {
                    (x0, y0) = (x0.min(x), y0.min(y));
                }
            }
        }
        assert!((x0 as f32 - line.x).abs() <= 3.0 && (y0 as f32 - line.y).abs() <= 3.0, "{x0},{y0} vs {line:?}");
        // One undo step.
        s.undo();
        assert!(s.doc.objects.is_empty());
    }

    #[test]
    fn colours_and_size_can_be_given() {
        let (mut s, line) = session();
        let out = s
            .execute("image.editText", &json!({"line": {"text": "x", "rect": line}, "color": "#FF0000", "background": "#000000", "fontSize": 40}))
            .unwrap();
        assert_eq!((out["color"].as_str(), out["background"].as_str(), out["fontSize"].as_f64()), (Some("#FF0000"), Some("#000000"), Some(40.0)));
    }

    #[test]
    fn junk_is_refused() {
        let (mut s, _) = session();
        for p in [
            json!({}),
            json!({"line": 5}),
            json!({"line": {"text": "x", "rect": {"x": 900, "y": 0, "w": 5, "h": 5}}}),
            json!({"line": {"text": "x", "rect": {"x": 1, "y": 1, "w": 5, "h": 5}}, "text": 7}),
        ] {
            assert!(s.execute("image.editText", &p).is_err(), "{p}");
        }
        assert!(s.doc.objects.is_empty());
    }
}
