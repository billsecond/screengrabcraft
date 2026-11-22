//! Text for annotations: the bundled fonts, line breaking, and glyph outlines as paths (so
//! text is filled, outlined and shadowed like any other shape, at any size).
//!
//! The fonts are PatroneOS Sans (regular and semibold) and PatroneOS Mono, under the SIL Open
//! Font Licence (`assets/fonts/`, listed in ATTRIBUTION.md). Italic is the upright face slanted. A style may
//! name a font on the computer instead: the host says where those are ([`FontSource`]); they
//! are read when a text needs one and never shipped, and where one is missing the text is drawn
//! in the bundled font.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use ab_glyph::{Font, FontArc, FontVec, GlyphId, OutlineCurve, PxScale, ScaleFont};
use screengrabcraft_model::{Pt, Rect, Style, TextAlign};
use tiny_skia::{Path, PathBuilder, Transform};

static SANS: &[u8] = include_bytes!("../../../assets/fonts/PatroneOSSans-Regular.ttf");
static SANS_BOLD: &[u8] = include_bytes!("../../../assets/fonts/PatroneOSSans-SemiBold.ttf");
static MONO: &[u8] = include_bytes!("../../../assets/fonts/PatroneOSMono-Regular.ttf");

/// The bundled fonts, by name (for showing text as it will be drawn while it is typed).
pub fn font_files() -> [(&'static str, &'static [u8]); 3] {
    [("sans", SANS), ("sans-bold", SANS_BOLD), ("mono", MONO)]
}

struct Fonts {
    regular: Option<FontArc>,
    bold: Option<FontArc>,
    mono: Option<FontArc>,
}

fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| Fonts {
        regular: FontArc::try_from_slice(SANS).ok(),
        bold: FontArc::try_from_slice(SANS_BOLD).ok(),
        mono: FontArc::try_from_slice(MONO).ok(),
    })
}

/// Where the fonts on the computer come from: the host finds them (the desktop does; the browser
/// has none). They are used, never shipped.
pub trait FontSource: Send + Sync {
    /// The file of the font `family` (its bold face when asked and there is one), and the face's
    /// number in it; none when the computer has no such font.
    fn face(&self, family: &str, bold: bool) -> Option<(Vec<u8>, u32)>;

    /// The families of the fonts on the computer, by name.
    fn families(&self) -> Vec<String>;
}

static SOURCE: OnceLock<Box<dyn FontSource>> = OnceLock::new();

/// The most fonts of the computer kept read at once.
const MAX_LOADED: usize = 32;

/// Fonts of the computer read so far, by family and boldness: none for one that isn't there.
type Loaded = HashMap<(String, bool), Option<FontArc>>;

fn loaded() -> &'static Mutex<Loaded> {
    static LOADED: OnceLock<Mutex<Loaded>> = OnceLock::new();
    LOADED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Sets where the fonts on the computer come from; once (false when one was set already).
pub fn set_font_source(source: Box<dyn FontSource>) -> bool {
    SOURCE.set(source).is_ok()
}

/// The families of the fonts on the computer; none where there is no way to them (the browser).
pub fn font_families() -> Option<Vec<String>> {
    SOURCE.get().map(|s| s.families())
}

/// The file of a font on the computer and its face's number, to show text being typed in it.
pub fn font_file(family: &str, bold: bool) -> Option<(Vec<u8>, u32)> {
    SOURCE.get()?.face(family, bold)
}

/// Whether there is a way to the fonts on the computer (not in the browser).
pub fn has_font_source() -> bool {
    SOURCE.get().is_some()
}

/// A font of the computer, read once.
fn system_font(family: &str, bold: bool) -> Option<FontArc> {
    let source = SOURCE.get()?;
    let key = (family.to_string(), bold);
    let mut map = loaded().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(f) = map.get(&key) {
        return f.clone();
    }
    if map.len() >= MAX_LOADED {
        map.clear();
    }
    let font = source.face(family, bold).and_then(|(bytes, index)| FontVec::try_from_vec_and_index(bytes, index).ok()).map(FontArc::new);
    map.insert(key, font.clone());
    font
}

/// The font a style's text is drawn in: the computer's font it names when there is one, else
/// the bundled ones (the typewriter's, bold or regular).
fn font_for(style: &Style) -> Option<FontArc> {
    if !style.font.is_bundled()
        && let Some(f) = system_font(style.font.as_str(), style.bold)
    {
        return Some(f);
    }
    let f = fonts();
    if style.mono {
        f.mono.clone().or_else(|| f.regular.clone())
    } else if style.bold {
        f.bold.clone().or_else(|| f.regular.clone())
    } else {
        f.regular.clone()
    }
}

/// One laid-out line: its glyphs with their pen positions (relative to the line's left end and
/// baseline) and its width.
#[derive(Clone, Debug, Default)]
pub struct Line {
    pub glyphs: Vec<(GlyphId, f32)>,
    pub width: f32,
}

/// Text broken into lines for a width.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub lines: Vec<Line>,
    /// Distance between baselines.
    pub line_height: f32,
    pub ascent: f32,
    /// The widest line.
    pub width: f32,
    pub height: f32,
}

/// The most lines laid out (text past them is not drawn).
const MAX_LINES: usize = 2_000;

/// The padding between a text box's edge and its text.
pub fn padding(style: &Style) -> f32 {
    (style.font_size * 0.35).max(4.0)
}

/// Lays `text` out in lines no wider than `max_width` (breaking at spaces, or inside a word
/// that is longer than a line). Without a width, lines break only at newlines.
pub fn layout(text: &str, style: &Style, max_width: Option<f32>) -> Layout {
    let Some(font) = font_for(style) else { return Layout::default() };
    let size = style.font_size.clamp(screengrabcraft_model::object::MIN_FONT, screengrabcraft_model::object::MAX_FONT);
    let scaled = font.as_scaled(PxScale::from(size));
    let line_height = (scaled.ascent() - scaled.descent() + scaled.line_gap()).max(1.0);
    let max_width = max_width.filter(|w| w.is_finite() && *w > size * 0.5);
    let mut lines: Vec<Line> = Vec::new();
    'paragraphs: for paragraph in text.split('\n') {
        let paragraph = paragraph.trim_end_matches('\r');
        let mut line = Line::default();
        let mut last: Option<GlyphId> = None;
        // Where the line could be broken: the glyph index after the last space, and the width there.
        let mut break_at: Option<(usize, f32)> = None;
        for ch in paragraph.chars() {
            let ch = if ch == '\t' { ' ' } else { ch };
            if ch.is_control() {
                continue;
            }
            let id = font.glyph_id(ch);
            let kern = last.map_or(0.0, |prev| scaled.kern(prev, id));
            let advance = scaled.h_advance(id);
            let x = line.width + kern;
            if let Some(limit) = max_width
                && ch != ' '
                && x + advance > limit
                && !line.glyphs.is_empty()
            {
                // Break: at the last space if there is one, else here.
                let (cut, _) = break_at.unwrap_or((line.glyphs.len(), line.width));
                let rest: Vec<(GlyphId, f32)> = line.glyphs.split_off(cut.min(line.glyphs.len()));
                let space = font.glyph_id(' ');
                while line.glyphs.last().is_some_and(|(g, _)| *g == space) {
                    line.glyphs.pop();
                }
                line.width = line.glyphs.last().map_or(0.0, |(g, x)| x + scaled.h_advance(*g));
                lines.push(std::mem::take(&mut line));
                if lines.len() >= MAX_LINES {
                    break 'paragraphs;
                }
                // The carried-over glyphs start the next line.
                let shift = rest.first().map_or(0.0, |(_, x)| *x);
                for (g, gx) in rest {
                    line.glyphs.push((g, gx - shift));
                }
                line.width = line.glyphs.last().map_or(0.0, |(g, x)| x + scaled.h_advance(*g));
                break_at = None;
                let x = line.width + last.map_or(0.0, |prev| scaled.kern(prev, id));
                line.glyphs.push((id, x));
                line.width = x + advance;
                last = Some(id);
                continue;
            }
            line.glyphs.push((id, x));
            line.width = x + advance;
            if ch == ' ' {
                break_at = Some((line.glyphs.len(), line.width));
            }
            last = Some(id);
        }
        lines.push(line);
        if lines.len() >= MAX_LINES {
            break;
        }
    }
    let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
    let height = line_height * lines.len().max(1) as f32;
    Layout { lines, line_height, ascent: scaled.ascent(), width, height }
}

/// The glyph outlines of laid-out text as one path, with the first line's top at `origin`,
/// each line placed by `align` inside `box_width`.
pub fn text_path(layout: &Layout, style: &Style, origin: Pt, box_width: f32) -> Option<Path> {
    let font = font_for(style)?;
    let size = style.font_size.clamp(screengrabcraft_model::object::MIN_FONT, screengrabcraft_model::object::MAX_FONT);
    let scaled = font.as_scaled(PxScale::from(size));
    let (sx, sy) = (scaled.h_scale_factor(), scaled.v_scale_factor());
    let mut pb = PathBuilder::new();
    let slant = if style.italic { 0.2 } else { 0.0 };
    for (i, line) in layout.lines.iter().enumerate() {
        let dx = match style.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => ((box_width - line.width) / 2.0).max(0.0),
            TextAlign::Right => (box_width - line.width).max(0.0),
        };
        let baseline = origin.y + layout.ascent + layout.line_height * i as f32;
        for (id, gx) in &line.glyphs {
            let Some(outline) = font.outline(*id) else { continue };
            let ox = origin.x + dx + gx;
            // Font units, y up → pixels, y down (and slanted for italic).
            let map = |p: ab_glyph::Point| {
                let (x, y) = (p.x * sx, p.y * sy);
                (ox + x + y * slant, baseline - y)
            };
            let mut pen: Option<ab_glyph::Point> = None;
            for curve in &outline.curves {
                let (start, end) = match curve {
                    OutlineCurve::Line(a, b) => (*a, *b),
                    OutlineCurve::Quad(a, _, b) => (*a, *b),
                    OutlineCurve::Cubic(a, _, _, b) => (*a, *b),
                };
                if pen.is_none_or(|p| (p.x - start.x).abs() > 0.01 || (p.y - start.y).abs() > 0.01) {
                    if pen.is_some() {
                        pb.close();
                    }
                    let (x, y) = map(start);
                    pb.move_to(x, y);
                }
                match curve {
                    OutlineCurve::Line(_, b) => {
                        let (x, y) = map(*b);
                        pb.line_to(x, y);
                    }
                    OutlineCurve::Quad(_, c, b) => {
                        let (cx, cy) = map(*c);
                        let (x, y) = map(*b);
                        pb.quad_to(cx, cy, x, y);
                    }
                    OutlineCurve::Cubic(_, c1, c2, b) => {
                        let (c1x, c1y) = map(*c1);
                        let (c2x, c2y) = map(*c2);
                        let (x, y) = map(*b);
                        pb.cubic_to(c1x, c1y, c2x, c2y, x, y);
                    }
                }
                pen = Some(end);
            }
            if pen.is_some() {
                pb.close();
            }
        }
    }
    pb.finish()
}

/// The box a text annotation needs for its text at a width (its padding included): at least
/// `min_width` wide, and as tall as its lines.
pub fn fit_box(text: &str, style: &Style, at: Pt, width: Option<f32>) -> Rect {
    let pad = padding(style);
    let lay = layout(text, style, width.map(|w| w - 2.0 * pad));
    let w = width.unwrap_or(lay.width + 2.0 * pad).max(style.font_size + 2.0 * pad);
    Rect::new(at.x, at.y, w, lay.height + 2.0 * pad)
}

/// A transform that moves a path by `d`.
pub fn shift(d: Pt) -> Transform {
    Transform::from_translate(d.x, d.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_load() {
        let f = fonts();
        assert!(f.regular.is_some() && f.bold.is_some() && f.mono.is_some());
    }

    /// A computer with one font, "Test Mono" (the bundled typewriter's file under another name).
    struct OneFont;

    impl FontSource for OneFont {
        fn face(&self, family: &str, _bold: bool) -> Option<(Vec<u8>, u32)> {
            (family == "Test Mono").then(|| (MONO.to_vec(), 0))
        }

        fn families(&self) -> Vec<String> {
            vec!["Test Mono".to_string()]
        }
    }

    /// Text in a font on the computer is laid out and drawn in it; a font the computer hasn't
    /// falls back to the bundled one.
    #[test]
    fn a_font_on_the_computer_is_used_and_a_missing_one_falls_back() {
        set_font_source(Box::new(OneFont));
        assert_eq!(font_families(), Some(vec!["Test Mono".to_string()]));
        let sans = Style { font_size: 30.0, ..Style::default() };
        let mono = Style { font: screengrabcraft_model::FontName::new("Test Mono"), ..sans };
        let missing = Style { font: screengrabcraft_model::FontName::new("Nowhere Sans"), ..sans };
        let w = |s: &Style| layout("iiii", s, None).width;
        assert!(w(&mono) > w(&sans) * 1.5, "typewriter letters are wider: {} {}", w(&mono), w(&sans));
        assert_eq!(w(&missing), w(&sans), "the bundled font where the computer hasn't it");
        assert_eq!(w(&Style { bold: true, ..mono }), w(&mono), "a font without a bold face keeps its letters");
        let path = text_path(&layout("ii", &mono, None), &mono, Pt::ZERO, 200.0).unwrap();
        assert!(path.bounds().width() > 20.0);
        assert!(font_file("Test Mono", false).is_some() && font_file("Nowhere Sans", false).is_none());
    }

    #[test]
    fn wraps_at_spaces_and_inside_long_words() {
        let style = Style { font_size: 20.0, ..Style::default() };
        let one = layout("hello world", &style, None);
        assert_eq!(one.lines.len(), 1);
        let narrow = layout("hello world", &style, Some(one.width * 0.7));
        assert_eq!(narrow.lines.len(), 2, "broken at the space");
        assert!(narrow.width <= one.width * 0.7 + 0.5);
        let word = layout("abcdefghijklmnopqrstuvwxyz", &style, Some(60.0));
        assert!(word.lines.len() > 2, "a long word is broken too");
        assert!(word.lines.iter().all(|l| l.width <= 60.0 + 0.5));
        assert_eq!(layout("a\nb\n\nc", &style, None).lines.len(), 4);
        assert_eq!(layout("", &style, Some(100.0)).lines.len(), 1);
    }

    #[test]
    fn paths_exist_for_visible_text() {
        let style = Style { font_size: 30.0, ..Style::default() };
        let lay = layout("Ag", &style, None);
        let path = text_path(&lay, &style, Pt::new(10.0, 10.0), lay.width).unwrap();
        let b = path.bounds();
        assert!(b.left() >= 9.0 && b.top() >= 9.0 && b.bottom() <= 10.0 + lay.height + 1.0, "{b:?}");
        assert!(text_path(&layout("   ", &style, None), &style, Pt::ZERO, 10.0).is_none());
    }

    #[test]
    fn hostile_text_is_fine() {
        let style = Style::default();
        let long = "x".repeat(100_000);
        let lay = layout(&long, &style, Some(50.0));
        assert!(lay.lines.len() <= MAX_LINES);
        let weird = "\u{0}\u{7}\t\u{202e}🙂\u{feff}";
        let _ = text_path(&layout(weird, &style, Some(1.0)), &style, Pt::ZERO, 1.0);
        let _ = layout("abc", &Style { font_size: f32::NAN, ..style }, Some(f32::INFINITY));
    }
}
