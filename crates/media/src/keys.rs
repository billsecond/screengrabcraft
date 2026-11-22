//! The shortcuts pressed while recording (which are kept, and which never are, is the
//! recorder's: see `screengrabcraft_capture::keys`), and how the video shows them: as keys drawn
//! on the picture for a moment after each press, where the screen's clips play that moment
//! (after cuts, speed changes and held frames).

use screengrabcraft_model::{Color, Pt, Raster, Rect, Style, TextAlign};
use serde::{Deserialize, Serialize};

use crate::recording::StreamKind;
use crate::timeline::Timeline;

/// The most presses a recording keeps.
pub const MAX_PRESSES: usize = 100_000;
/// The longest a shortcut's words are.
pub const MAX_WORDS: usize = 48;
/// How long a shortcut stays on the video after it's pressed (µs), the last of it fading.
pub const SHOWN_US: u64 = 1_500_000;
const FADE_US: u64 = 250_000;

/// A shortcut pressed while recording: when (µs of the recording) and its keys, `+` between
/// them (`Ctrl+Shift+S`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Press {
    pub t: u64,
    pub keys: String,
}

/// A shortcut's words as they're kept: one clean line, not too long; none if empty.
fn clean(words: &str) -> Option<String> {
    let s: String = words.chars().filter(|c| !c.is_control()).take(MAX_WORDS).collect();
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// A press as the recording file keeps it: when, then its words.
pub fn press_bytes(p: &Press) -> Vec<u8> {
    let words = clean(&p.keys).unwrap_or_default();
    let mut b = Vec::with_capacity(8 + words.len());
    b.extend_from_slice(&p.t.to_le_bytes());
    b.extend_from_slice(words.as_bytes());
    b
}

/// A press read from a recording file; none if it isn't one.
pub fn press_from(b: &[u8]) -> Option<Press> {
    let t = u64::from_le_bytes(b.get(..8)?.try_into().ok()?);
    let words = b.get(8..).filter(|w| w.len() <= MAX_WORDS * 4)?;
    Some(Press { t, keys: clean(std::str::from_utf8(words).ok()?)? })
}

/// Where on the picture the keys are shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Place {
    TopLeft,
    Top,
    TopRight,
    BottomLeft,
    /// The middle at the bottom, above where captions go.
    #[default]
    Bottom,
    BottomRight,
}

impl Place {
    pub const ALL: [Place; 6] = [Place::TopLeft, Place::Top, Place::TopRight, Place::BottomLeft, Place::Bottom, Place::BottomRight];

    pub fn label(self) -> &'static str {
        match self {
            Place::TopLeft => "Top left",
            Place::Top => "Top",
            Place::TopRight => "Top right",
            Place::BottomLeft => "Bottom left",
            Place::Bottom => "Bottom",
            Place::BottomRight => "Bottom right",
        }
    }
}

/// How the keys pressed are shown on the video: at all, where, how large, light or dark keys.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Look {
    pub show: bool,
    pub place: Place,
    /// 0.5 to 2.5 (1 is a key about a fourteenth of the picture's height).
    pub size: f32,
    /// Light keys with dark words, else dark keys with light words.
    pub light: bool,
}

impl Default for Look {
    fn default() -> Look {
        Look { show: false, place: Place::default(), size: 1.0, light: false }
    }
}

impl Look {
    /// Its numbers in range (a file's junk made sensible).
    pub fn checked(self) -> Look {
        Look { size: if self.size.is_finite() { self.size.clamp(0.5, 2.5) } else { 1.0 }, ..self }
    }
}

/// The shortcut showing at timeline time `t` (the last pressed in the moment before, where the
/// screen's clips play it), and how much of it shows (0 to 1, fading at the end). None when the
/// screen is hidden or nothing was pressed then.
pub fn showing<'a>(presses: &'a [Press], timeline: &Timeline, t: u64) -> Option<(&'a str, f32)> {
    let track = timeline.find(StreamKind::Screen).and_then(|i| timeline.tracks.get(i)).filter(|tr| !tr.muted)?;
    let lo = t.saturating_sub(SHOWN_US);
    let mut best: Option<(u64, &Press)> = None;
    for c in &track.clips {
        if c.hold || c.reverse || c.end() <= lo || c.at > t {
            continue;
        }
        // The part of the recording this clip plays between `lo` and `t`.
        let (a, b) = (c.at.max(lo), c.end().min(t.saturating_add(1)));
        let (sa, sb) = (c.source_at(a - c.at), c.source_at(b - c.at));
        let k = presses.partition_point(|p| p.t < sb);
        if let Some(p) = k.checked_sub(1).and_then(|i| presses.get(i)).filter(|p| p.t >= sa) {
            let at = c.at.saturating_add(c.played(p.t.saturating_sub(c.from)));
            if at <= t && best.is_none_or(|(b, _)| at > b) {
                best = Some((at, p));
            }
        }
    }
    let (at, p) = best?;
    let age = t - at;
    if age >= SHOWN_US {
        return None;
    }
    let left = SHOWN_US - age;
    Some((p.keys.as_str(), if left < FADE_US { left as f32 / FADE_US as f32 } else { 1.0 }))
}

/// Draws a shortcut on the picture as keys (`Ctrl` `Shift` `S`, a plus between), where and as
/// the look says, `alpha` of it (0 to 1).
pub fn draw(out: &mut Raster, keys: &str, look: Look, alpha: f32) {
    use screengrabcraft_render::{draw, text};
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 || out.width() < 16 || out.height() < 16 {
        return;
    }
    let (w, h) = (out.width() as f32, out.height() as f32);
    let cap_h = (h * 0.072 * look.checked().size).clamp(18.0, 400.0);
    let font = cap_h * 0.44;
    let (pad, gap) = (cap_h * 0.34, cap_h * 0.5);
    let (ink, face, lip) = if look.light {
        (Color::rgb(0x22, 0x24, 0x2A), Color::rgb(0xFA, 0xFA, 0xFC), Color::rgb(0xB8, 0xBC, 0xC6))
    } else {
        (Color::WHITE, Color::rgb(0x22, 0x23, 0x29), Color::rgb(0x0C, 0x0C, 0x10))
    };
    let style = Style { font_size: font, bold: true, text_color: ink, align: TextAlign::Left, ..Style::default() };
    let words: Vec<&str> = keys.split('+').map(str::trim).filter(|k| !k.is_empty()).take(6).collect();
    let laid: Vec<_> = words.iter().map(|k| text::layout(k, &style, None)).filter(|l| !l.lines.is_empty()).collect();
    if laid.is_empty() {
        return;
    }
    let widths: Vec<f32> = laid.iter().map(|l| (l.width + 2.0 * pad).max(cap_h)).collect();
    let whole = widths.iter().sum::<f32>() + gap * (widths.len() - 1) as f32;
    let scale = if whole > w * 0.94 { w * 0.94 / whole } else { 1.0 };
    let margin = h * 0.04;
    let left = match look.place {
        Place::TopLeft | Place::BottomLeft => margin,
        Place::Top | Place::Bottom => (w - whole * scale) / 2.0,
        Place::TopRight | Place::BottomRight => w - margin - whole * scale,
    };
    let top = match look.place {
        Place::TopLeft | Place::Top | Place::TopRight => margin,
        // The middle one sits above where captions go.
        Place::Bottom => h * 0.8 - cap_h * scale,
        Place::BottomLeft | Place::BottomRight => h - margin - cap_h * scale * 1.08,
    }
    .clamp(0.0, (h - cap_h * scale * 1.1).max(0.0));
    // Only the rows the keys cover are drawn on.
    let (y0, y1) = (top.floor().max(0.0) as u32, ((top + cap_h * scale * 1.12).ceil() as u32).min(out.height()));
    let row = out.width() as usize * 4;
    let (from, to) = (y0 as usize * row, y1 as usize * row);
    let Some(band) = out.pixels().get(from..to).map(<[u8]>::to_vec) else { return };
    let Some(size) = tiny_skia::IntSize::from_wh(out.width(), y1.saturating_sub(y0)) else { return };
    let Some(mut canvas) = tiny_skia::Pixmap::from_vec(band, size) else { return };
    let shift = tiny_skia::Transform::from_scale(scale, scale).post_translate(left, top - y0 as f32);
    let paint = |c: Color, a: f32| {
        let mut p = tiny_skia::Paint::default();
        p.set_color(draw::skia_color(c, a * alpha));
        p.anti_alias = true;
        p
    };
    let radius = cap_h * 0.2;
    let mut x = 0.0;
    for (k, (lay, kw)) in laid.iter().zip(&widths).enumerate() {
        if k > 0 {
            let plus = text::layout("+", &style, None);
            if let Some(path) = text::text_path(&plus, &style, Pt::new(x - gap / 2.0 - plus.width / 2.0, (cap_h - plus.height) / 2.0), plus.width) {
                let shade = if look.light { Color::rgb(0x22, 0x24, 0x2A) } else { Color::WHITE };
                // On any picture: a soft dark (or light) edge round the plus.
                let mut edge = paint(if look.light { Color::WHITE } else { Color::BLACK }, 0.55);
                edge.anti_alias = true;
                let stroke = tiny_skia::Stroke { width: cap_h * 0.08, ..tiny_skia::Stroke::default() };
                canvas.stroke_path(&path, &edge, &stroke, shift, None);
                canvas.fill_path(&path, &paint(shade, 1.0), tiny_skia::FillRule::Winding, shift, None);
            }
        }
        // The key: its lip under it, its face, its words in the middle.
        if let Some(r) = draw::rounded_rect(Rect::new(x, cap_h * 0.08, *kw, cap_h), radius) {
            canvas.fill_path(&r, &paint(lip, 0.92), tiny_skia::FillRule::Winding, shift, None);
        }
        if let Some(r) = draw::rounded_rect(Rect::new(x, 0.0, *kw, cap_h), radius) {
            canvas.fill_path(&r, &paint(face, 0.92), tiny_skia::FillRule::Winding, shift, None);
            let rim = tiny_skia::Stroke { width: (cap_h * 0.03).max(1.0), ..tiny_skia::Stroke::default() };
            canvas.stroke_path(&r, &paint(if look.light { lip } else { Color::rgb(0x5A, 0x5C, 0x66) }, 0.9), &rim, shift, None);
        }
        if let Some(path) = text::text_path(lay, &style, Pt::new(x + (kw - lay.width) / 2.0, (cap_h - lay.height) / 2.0), lay.width) {
            canvas.fill_path(&path, &paint(ink, 1.0), tiny_skia::FillRule::Winding, shift, None);
        }
        x += kw + gap;
    }
    if let Some(dst) = out.pixels_mut().get_mut(from..to) {
        dst.copy_from_slice(canvas.data());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{CameraLayout, CaptionOptions, Clip, Track};

    fn screen(clips: Vec<Clip>) -> Timeline {
        Timeline {
            tracks: vec![Track {
                stream: 0,
                kind: StreamKind::Screen,
                muted: false,
                volume: 1.0,
                clips,
                name: None,
                denoise: 0.0,
                points: Vec::new(),
                locked: false,
                solo: false,
            }],
            camera: CameraLayout::default(),
            captions: CaptionOptions::default(),
            layers: Vec::new(),
            media: Vec::new(),
            pointer: Default::default(),
            corrected: None,
            markers: Vec::new(),
            keys: Look::default(),
            ducking: Default::default(),
        }
    }

    fn press(t: u64, keys: &str) -> Press {
        Press { t, keys: keys.to_string() }
    }

    #[test]
    fn a_press_is_kept_in_the_file_and_junk_isnt() {
        let p = press(1_234_567, "Ctrl+Shift+S");
        assert_eq!(press_from(&press_bytes(&p)), Some(p));
        assert_eq!(press_from(&[1, 2, 3]), None);
        assert_eq!(press_from(&[0; 8]), None, "no words");
        assert_eq!(press_from(&[0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFE]), None, "not text");
        assert_eq!(press_from(&[vec![0u8; 8], vec![b'x'; 9000]].concat()), None, "too long");
        assert_eq!(press_from(&[vec![0u8; 8], b"Ctrl+\nC\x07".to_vec()].concat()).map(|p| p.keys), Some("Ctrl+C".to_string()));
    }

    #[test]
    fn a_shortcut_shows_for_a_moment_where_the_edits_put_it() {
        let presses = vec![press(1_000_000, "Ctrl+C"), press(3_000_000, "Ctrl+V"), press(3_500_000, "Enter")];
        let mut tl = screen(vec![Clip::new(0, 0, 10_000_000)]);
        assert_eq!(showing(&presses, &tl, 500_000), None, "nothing yet");
        assert_eq!(showing(&presses, &tl, 1_200_000), Some(("Ctrl+C", 1.0)));
        let (_, fading) = showing(&presses, &tl, 2_400_000).unwrap();
        assert!(fading > 0.0 && fading < 1.0, "{fading}");
        assert_eq!(showing(&presses, &tl, 2_600_000), None, "gone after a second and a half");
        assert_eq!(showing(&presses, &tl, 3_600_000).map(|s| s.0), Some("Enter"), "the newest");
        // The first two seconds cut: Ctrl+V comes at 1 s.
        tl.cut(0, 2_000_000).unwrap();
        assert_eq!(showing(&presses, &tl, 1_100_000).map(|s| s.0), Some("Ctrl+V"));
        assert_eq!(showing(&presses, &tl, 200_000), None, "Ctrl+C was cut");
        // Twice as fast: at half a second.
        tl.set_speed(0, 8_000_000, 200, None).unwrap();
        assert_eq!(showing(&presses, &tl, 550_000).map(|s| s.0), Some("Ctrl+V"));
        // The screen hidden: none.
        tl.set_muted(0, true).unwrap();
        assert_eq!(showing(&presses, &tl, 550_000), None);
        assert_eq!(showing(&[], &screen(vec![]), 0), None);
    }

    #[test]
    fn keys_are_drawn_where_the_look_says_and_never_panic() {
        let grey = Color::rgb(0x80, 0x80, 0x80);
        let changed = |pic: &Raster, rows: std::ops::Range<u32>, cols: std::ops::Range<u32>| {
            rows.flat_map(|y| cols.clone().map(move |x| (x, y))).filter(|(x, y)| pic.pixel(*x, *y) != Some(grey)).count()
        };
        for place in Place::ALL {
            let mut pic = Raster::filled(640, 360, grey).unwrap();
            draw(&mut pic, "Ctrl+Shift+S", Look { show: true, place, size: 1.0, light: false }, 1.0);
            let (top, bottom) = (changed(&pic, 0..180, 0..640), changed(&pic, 180..360, 0..640));
            let (left, right) = (changed(&pic, 0..360, 0..213), changed(&pic, 0..360, 427..640));
            let up = matches!(place, Place::TopLeft | Place::Top | Place::TopRight);
            assert!(if up { top > 500 && bottom == 0 } else { bottom > 500 && top == 0 }, "{place:?}: {top} {bottom}");
            match place {
                Place::TopLeft | Place::BottomLeft => assert!(left > right, "{place:?}"),
                Place::TopRight | Place::BottomRight => assert!(right > left, "{place:?}"),
                _ => {}
            }
        }
        // Faded out: nothing drawn. Light keys: light pixels.
        let mut pic = Raster::filled(640, 360, grey).unwrap();
        draw(&mut pic, "Enter", Look::default(), 0.0);
        assert_eq!(changed(&pic, 0..360, 0..640), 0);
        draw(&mut pic, "Enter", Look { light: true, ..Look::default() }, 1.0);
        assert!(pic.pixels().as_chunks::<4>().0.iter().any(|p| p[0] > 240));
        // A look for the eye (SGC_SHOTS=<folder>).
        if let Ok(dir) = std::env::var("SGC_SHOTS") {
            let mut pic = Raster::filled(1280, 720, Color::rgb(0xE8, 0xEC, 0xF2)).unwrap();
            draw(&mut pic, "Ctrl+Shift+S", Look::default(), 1.0);
            draw(&mut pic, "Alt+F4", Look { place: Place::TopRight, light: true, ..Look::default() }, 1.0);
            let png = screengrabcraft_render::encode(&pic, screengrabcraft_model::ImageFormat::Png, 90).unwrap();
            std::fs::write(format!("{dir}/keys-pressed.png"), png).unwrap();
        }
        // Any picture, any words.
        for (w, h) in [(1, 1), (16, 16), (17, 3000), (3000, 17)] {
            let mut pic = Raster::filled(w, h, Color::BLACK).unwrap();
            draw(&mut pic, &"Ctrl+".repeat(40), Look { size: 99.0, ..Look::default() }, 1.0);
            draw(&mut pic, "+++", Look::default(), 1.0);
        }
        assert_eq!(Look { size: f32::NAN, ..Look::default() }.checked().size, 1.0);
    }
}
