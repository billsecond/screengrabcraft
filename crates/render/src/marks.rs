//! Marks written on the picture: a watermark (words or a picture, over the picture or under what
//! is on it) and the capture's details (when it was taken, of what, its size).

use screengrabcraft_model::{Color, Document, FontName, Object, ObjectId, Pt, Raster, RasterError, Rect, Shape, Style};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

use crate::{canvas, draw, effects, text};

fn bad(r: &Raster) -> RasterError {
    RasterError::Empty(r.width(), r.height())
}

fn bounded(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { default }
}

/// Where on the picture a watermark goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Place {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
    /// Again and again, all over it.
    Tiled,
}

impl Place {
    pub const ALL: [Place; 10] = [
        Place::Center,
        Place::Tiled,
        Place::TopLeft,
        Place::Top,
        Place::TopRight,
        Place::Left,
        Place::Right,
        Place::BottomLeft,
        Place::Bottom,
        Place::BottomRight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Place::TopLeft => "Top left",
            Place::Top => "Top",
            Place::TopRight => "Top right",
            Place::Left => "Left",
            Place::Center => "Middle",
            Place::Right => "Right",
            Place::BottomLeft => "Bottom left",
            Place::Bottom => "Bottom",
            Place::BottomRight => "Bottom right",
            Place::Tiled => "All over",
        }
    }

    /// Where along each side, 0 to 1 (the middle for tiles).
    fn anchor(self) -> (f32, f32) {
        match self {
            Place::TopLeft => (0.0, 0.0),
            Place::Top => (0.5, 0.0),
            Place::TopRight => (1.0, 0.0),
            Place::Left => (0.0, 0.5),
            Place::Center | Place::Tiled => (0.5, 0.5),
            Place::Right => (1.0, 0.5),
            Place::BottomLeft => (0.0, 1.0),
            Place::Bottom => (0.5, 1.0),
            Place::BottomRight => (1.0, 1.0),
        }
    }
}

/// What a watermark is.
#[derive(Clone, Debug)]
pub enum Mark<'a> {
    Words { text: &'a str, color: Color, bold: bool, font: FontName },
    Picture(&'a Raster),
}

/// A watermark: what it is, where, how large (`size`, 2 to 100 hundredths of the picture's
/// width), turned by `angle` degrees (-90 to 90, the way a clock goes), `opacity` (0 to 1), and
/// whether it goes under what is on the picture.
#[derive(Clone, Debug)]
pub struct Watermark<'a> {
    pub mark: Mark<'a>,
    pub place: Place,
    pub size: f32,
    pub angle: f32,
    pub opacity: f32,
    /// Under: it shows on the picture's plain parts and where it is see-through, and stays out
    /// from under its words and lines, as if printed on the paper first. Over: on top of it all.
    pub under: bool,
}

/// The longest watermark or capture details text drawn.
pub const MAX_MARK_TEXT: usize = 400;

/// A text annotation for `text` in `style`, its box at `at`.
fn text_object(text: &str, style: Style, at: Pt) -> Object {
    let rect = text::fit_box(text, &style, at, None);
    Object { id: ObjectId(1), shape: Shape::Text { rect, text: text.to_string() }, style }
}

/// The mark drawn on its own, `width` pixels wide at most and `height` tall.
fn mark_pixmap(mark: &Mark<'_>, width: f32, height: f32) -> Option<Pixmap> {
    match mark {
        Mark::Words { text, color, bold, font } => {
            let text: String = text.chars().take(MAX_MARK_TEXT).collect();
            if text.trim().is_empty() {
                return None;
            }
            let probe = Style { font_size: 100.0, bold: *bold, font: *font, ..Style::default() };
            let lay = text::layout(&text, &probe, None);
            if lay.width <= 0.0 || lay.height <= 0.0 {
                return None;
            }
            let size = (100.0 * (width / lay.width).min(height / lay.height))
                .clamp(screengrabcraft_model::object::MIN_FONT, screengrabcraft_model::object::MAX_FONT);
            let style = Style {
                font_size: size,
                bold: *bold,
                font: *font,
                text_color: *color,
                // A faint line of the other colour round the letters, so they read on anything.
                text_outline: color.contrasting().with_alpha(90),
                text_outline_width: (size / 24.0).max(1.0),
                stroke: Color::TRANSPARENT,
                fill: Color::TRANSPARENT,
                width: 0.0,
                shadow: false,
                ..Style::default()
            };
            let o = text_object(&text, style, Pt::ZERO);
            let b = o.shape.rect()?;
            let mut pix = canvas::blank(b.w.ceil().max(1.0) as u32, b.h.ceil().max(1.0) as u32)?;
            let doc = Document::new(Raster::single(Color::TRANSPARENT));
            draw::draw_object(&mut pix, &doc, &o);
            Some(pix)
        }
        Mark::Picture(p) => {
            let (pw, ph) = (p.width().max(1) as f32, p.height().max(1) as f32);
            let f = (width / pw).min(height / ph);
            let (w, h) = ((pw * f).round().max(1.0) as u32, (ph * f).round().max(1.0) as u32);
            let scaled = effects::resize(p, w, h).ok()?;
            canvas::to_pixmap(&scaled)
        }
    }
}

/// How plain each pixel's neighbourhood is (1, plain, to 0, at words and lines), for a watermark
/// under what is on the picture.
fn plainness(r: &Raster) -> Option<Vec<f32>> {
    let (w, h) = (r.width() as usize, r.height() as usize);
    let lum: Vec<f32> =
        r.pixels().as_chunks::<4>().0.iter().map(|p| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2])).collect();
    let at = |x: usize, y: usize| lum.get(y * w + x).copied().unwrap_or(0.0);
    let mut edges = canvas::blank(r.width(), r.height())?;
    for (i, px) in edges.data_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (i % w.max(1), i / w.max(1));
        let c = at(x, y);
        let mut e = 0.0f32;
        for (nx, ny) in [(x.saturating_sub(1), y), ((x + 1).min(w - 1), y), (x, y.saturating_sub(1)), (x, (y + 1).min(h - 1))] {
            e = e.max((c - at(nx, ny)).abs());
        }
        let v = (e * 4.0).clamp(0.0, 255.0) as u8;
        *px = [v, v, v, 255];
    }
    // Spread, so the mark keeps a little away from them too.
    canvas::blur(&mut edges, 2.5);
    Some(
        edges
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let t = ((f32::from(p[0]) / 255.0 - 0.04) / 0.25).clamp(0.0, 1.0);
                1.0 - t * t * (3.0 - 2.0 * t)
            })
            .collect(),
    )
}

/// The picture with a watermark on it. An error when there is nothing to draw (no words).
pub fn watermark(r: &Raster, wm: &Watermark<'_>) -> Result<Raster, RasterError> {
    let (w, h) = (r.width() as f32, r.height() as f32);
    let size = bounded(wm.size, 2.0, 100.0, 30.0) / 100.0;
    let tiled = wm.place == Place::Tiled;
    let mark = mark_pixmap(&wm.mark, (w * size).max(4.0), if tiled { h * 0.5 } else { h * 0.9 }.max(4.0)).ok_or(bad(r))?;
    let (mw, mh) = (mark.width() as f32, mark.height() as f32);
    let angle = bounded(wm.angle, -90.0, 90.0, 0.0);
    let (sin, cos) = angle.to_radians().sin_cos();
    let (bw, bh) = (mw * cos.abs() + mh * sin.abs(), mw * sin.abs() + mh * cos.abs());
    let margin = (w.min(h) * 0.03).max(4.0);
    let centres: Vec<Pt> = if tiled {
        let (sx, sy) = ((bw * 1.4).max(8.0), (bh * 2.2).max(8.0));
        let (cols, rows) = ((w / sx).ceil() as i64 + 2, (h / sy).ceil() as i64 + 2);
        // A few hundred at most.
        let (cols, rows) = (cols.min(40), rows.min(40));
        (-1..rows)
            .flat_map(|row| {
                let shift = if row.rem_euclid(2) == 1 { sx / 2.0 } else { 0.0 };
                (-1..cols).map(move |col| Pt::new(col as f32 * sx + shift + sx / 2.0, row as f32 * sy + sy / 2.0))
            })
            .collect()
    } else {
        let (ax, ay) = wm.place.anchor();
        let x = margin + bw / 2.0 + (w - 2.0 * margin - bw) * ax;
        let y = margin + bh / 2.0 + (h - 2.0 * margin - bh) * ay;
        vec![Pt::new(x, y)]
    };
    let mut layer = canvas::blank(r.width(), r.height()).ok_or(bad(r))?;
    let paint = PixmapPaint { quality: FilterQuality::Bilinear, ..PixmapPaint::default() };
    for c in centres {
        let t = Transform::from_translate(c.x, c.y).pre_rotate(angle).pre_translate(-mw / 2.0, -mh / 2.0);
        layer.draw_pixmap(0, 0, mark.as_ref(), &paint, t, None);
    }
    let opacity = bounded(wm.opacity, 0.0, 1.0, 0.3);
    let plain = if wm.under { plainness(r) } else { None };
    for (i, px) in layer.data_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let mut k = opacity;
        if let Some(p) = &plain {
            let see_through = 1.0 - r.pixels().get(i * 4 + 3).map_or(1.0, |a| f32::from(*a) / 255.0);
            k *= p.get(i).copied().unwrap_or(1.0).max(see_through);
        }
        if k < 1.0 {
            for c in px.iter_mut() {
                *c = (f32::from(*c) * k).round() as u8;
            }
        }
    }
    let mut out = canvas::to_pixmap(r).ok_or(bad(r))?;
    out.draw_pixmap(0, 0, layer.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    canvas::from_pixmap(&out).ok_or(bad(r))
}

/// Where a capture's details go: in a corner, or on a strip of their own under the picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DetailsPlace {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
    /// A strip added under the picture (it grows).
    Below,
}

impl DetailsPlace {
    pub const ALL: [DetailsPlace; 5] =
        [DetailsPlace::BottomRight, DetailsPlace::BottomLeft, DetailsPlace::TopRight, DetailsPlace::TopLeft, DetailsPlace::Below];

    pub fn label(self) -> &'static str {
        match self {
            DetailsPlace::TopLeft => "Top left",
            DetailsPlace::TopRight => "Top right",
            DetailsPlace::BottomLeft => "Bottom left",
            DetailsPlace::BottomRight => "Bottom right",
            DetailsPlace::Below => "On a strip below",
        }
    }
}

/// Words written on the picture in a box of `background`: in a corner, or on a strip of that
/// colour added under it. An error when there are none.
pub fn details(r: &Raster, words: &str, place: DetailsPlace, font_size: f32, color: Color, background: Color) -> Result<Raster, RasterError> {
    let words: String = words.chars().filter(|c| !c.is_control() || *c == '\n').take(MAX_MARK_TEXT).collect();
    if words.trim().is_empty() {
        return Err(bad(r));
    }
    let size = bounded(font_size, 8.0, 200.0, 14.0);
    let below = place == DetailsPlace::Below;
    let style = Style {
        font_size: size,
        text_color: color,
        fill: if below { Color::TRANSPARENT } else { background },
        stroke: Color::TRANSPARENT,
        width: 0.0,
        shadow: false,
        ..Style::default()
    };
    let probe = text::fit_box(&words, &style, Pt::ZERO, None);
    let (w, h) = (r.width() as f32, r.height() as f32);
    let margin = (size * 0.6).round();
    let (base, at) = if below {
        let strip = (probe.h + margin).ceil().max(1.0) as i32;
        (effects::canvas_size(r, 0, 0, 0, strip, background)?, Pt::new(margin, h + margin / 2.0))
    } else {
        let x = if matches!(place, DetailsPlace::TopLeft | DetailsPlace::BottomLeft) { margin } else { w - margin - probe.w };
        let y = if matches!(place, DetailsPlace::TopLeft | DetailsPlace::TopRight) { margin } else { h - margin - probe.h };
        (r.clone(), Pt::new(x.max(0.0), y.max(0.0)))
    };
    let mut pix = canvas::to_pixmap(&base).ok_or(bad(r))?;
    let o = text_object(&words, style, at);
    let doc = Document::new(Raster::single(Color::TRANSPARENT));
    draw::draw_object(&mut pix, &doc, &o);
    canvas::from_pixmap(&pix).ok_or(bad(r))
}

/// The box capture details take on a picture of this size (for tests and for what moves).
pub fn details_box(words: &str, font_size: f32) -> Rect {
    let style = Style { font_size: bounded(font_size, 8.0, 200.0, 14.0), ..Style::default() };
    text::fit_box(words, &style, Pt::ZERO, None)
}

/// A capture's time as people write it: `2026-11-21 14:05` from RFC 3339 text
/// (`2026-11-21T14:05:09+01:00`); the text as it is when it isn't that.
pub fn readable_time(rfc3339: &str) -> String {
    let t = rfc3339.trim();
    match (t.get(..10), t.get(10..11), t.get(11..16)) {
        (Some(day), Some("T" | " "), Some(hm)) if day.bytes().filter(u8::is_ascii_digit).count() == 8 && hm.as_bytes().get(2) == Some(&b':') => {
            format!("{day} {hm}")
        }
        _ => t.chars().take(60).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey() -> Raster {
        Raster::filled(200, 120, Color::rgb(90, 90, 90)).unwrap()
    }

    fn changed(a: &Raster, b: &Raster, area: Rect) -> usize {
        let mut n = 0;
        for y in area.y as u32..area.bottom() as u32 {
            for x in area.x as u32..area.right() as u32 {
                if a.pixel(x, y) != b.pixel(x, y) {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn words_go_where_they_are_put() {
        let r = grey();
        let words = Mark::Words { text: "DRAFT", color: Color::WHITE, bold: true, font: FontName::BUNDLED };
        let wm = Watermark { mark: words.clone(), place: Place::BottomRight, size: 30.0, angle: 0.0, opacity: 0.8, under: false };
        let out = watermark(&r, &wm).unwrap();
        assert_eq!((out.width(), out.height()), (200, 120));
        assert!(changed(&r, &out, Rect::new(100.0, 60.0, 100.0, 60.0)) > 50, "in the bottom right");
        assert_eq!(changed(&r, &out, Rect::new(0.0, 0.0, 100.0, 60.0)), 0, "and nowhere else");
        // All over, turned.
        let tiled = watermark(&r, &Watermark { place: Place::Tiled, angle: -30.0, size: 20.0, ..wm.clone() }).unwrap();
        for q in [Rect::new(0.0, 0.0, 100.0, 60.0), Rect::new(100.0, 60.0, 100.0, 60.0)] {
            assert!(changed(&r, &tiled, q) > 30, "{q:?}");
        }
        // No words, no watermark.
        let empty = Watermark { mark: Mark::Words { text: "  ", color: Color::WHITE, bold: false, font: FontName::BUNDLED }, ..wm };
        assert!(watermark(&r, &empty).is_err());
    }

    #[test]
    fn a_picture_watermark_and_one_under_the_words() {
        let logo = Raster::filled(40, 20, Color::RED).unwrap();
        // Half the picture plain grey, half dark lines on it.
        let mut r = grey();
        for y in (60..120).step_by(4) {
            for x in 0..200 {
                let i = (y * 200 + x) * 4;
                r.pixels_mut()[i..i + 3].copy_from_slice(&[250, 250, 250]);
            }
        }
        let wm = Watermark { mark: Mark::Picture(&logo), place: Place::Center, size: 90.0, angle: 0.0, opacity: 1.0, under: true };
        let out = watermark(&r, &wm).unwrap();
        let reddish = |c: Color| c.r > 200 && c.g < 80;
        assert!(reddish(out.pixel(100, 40).unwrap()), "on the plain part: {:?}", out.pixel(100, 40));
        assert!(!reddish(out.pixel(100, 80).unwrap()), "not over the lines: {:?}", out.pixel(100, 80));
        let over = watermark(&r, &Watermark { under: false, ..wm }).unwrap();
        assert!(reddish(over.pixel(100, 80).unwrap()), "over everything");
    }

    #[test]
    fn details_in_a_corner_or_on_a_strip() {
        let r = grey();
        let corner = details(&r, "2026-11-21 14:05 · 200 × 120", DetailsPlace::TopLeft, 12.0, Color::WHITE, Color::rgba(0, 0, 0, 160)).unwrap();
        assert_eq!((corner.width(), corner.height()), (200, 120));
        assert!(changed(&r, &corner, Rect::new(0.0, 0.0, 100.0, 30.0)) > 50);
        assert_eq!(changed(&r, &corner, Rect::new(0.0, 60.0, 200.0, 60.0)), 0);
        let strip = details(&r, "Notepad", DetailsPlace::Below, 14.0, Color::BLACK, Color::WHITE).unwrap();
        assert!(strip.height() > 120 && strip.width() == 200);
        assert_eq!(strip.pixel(150, 121), Some(Color::WHITE));
        assert_eq!(strip.pixel(10, 10), Some(Color::rgb(90, 90, 90)));
        assert!(details(&r, " \n ", DetailsPlace::Below, 14.0, Color::BLACK, Color::WHITE).is_err());
        assert!(details_box("x", f32::NAN).h > 0.0);
    }

    #[test]
    fn times_read_as_people_write_them() {
        assert_eq!(readable_time("2026-11-21T14:05:09+01:00"), "2026-11-21 14:05");
        assert_eq!(readable_time("yesterday"), "yesterday");
        assert_eq!(readable_time("2026-11-2"), "2026-11-2");
        assert_eq!(readable_time("é".repeat(100).as_str()).chars().count(), 60);
    }
}
