//! A watermark on the video: words, or a picture of the media bin, in a corner, as see-through
//! as asked, on every frame (so in every video made from it). It's set in Export Video's window.

use screengrabcraft_model::{Color, Pt, Raster, Style, TextAlign};
use screengrabcraft_render::{canvas, draw, text};
use serde::{Deserialize, Serialize};
use tiny_skia::{FillRule, Paint, PixmapPaint, Transform};

use crate::layer_draw::MediaStore;
use crate::layers::MediaKind;
use crate::timeline::{Corner, Timeline};

/// The longest words a watermark keeps.
pub const MAX_WORDS: usize = 80;

/// What's on every frame, and where.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Watermark {
    pub on: bool,
    /// The words shown (when no picture is).
    pub words: String,
    /// A picture of the media bin shown instead of the words (its id).
    pub picture: Option<u32>,
    pub corner: Corner,
    /// How much it shows: 0 not at all, 1 fully.
    pub opacity: f32,
    /// How tall it is, as a share of the video's height (0.02 to 0.3).
    pub size: f32,
}

impl Default for Watermark {
    fn default() -> Watermark {
        Watermark { on: false, words: String::new(), picture: None, corner: Corner::BottomRight, opacity: 0.6, size: 0.06 }
    }
}

/// The smallest and largest a watermark is (a share of the video's height).
pub const MIN_SIZE: f32 = 0.02;
pub const MAX_SIZE: f32 = 0.3;

impl Watermark {
    /// Every value in its range, the words one clean line.
    pub fn checked(self) -> Watermark {
        let words: String = self.words.chars().filter(|c| !c.is_control()).take(MAX_WORDS).collect();
        Watermark {
            words: words.trim().to_string(),
            opacity: if self.opacity.is_finite() { self.opacity.clamp(0.0, 1.0) } else { 0.6 },
            size: if self.size.is_finite() { self.size.clamp(MIN_SIZE, MAX_SIZE) } else { 0.06 },
            ..self
        }
    }

    /// Whether it shows anything: on, seen, and with words or a picture.
    pub fn shows(&self) -> bool {
        self.on && self.opacity > 0.0 && (self.picture.is_some() || !self.words.trim().is_empty())
    }
}

/// Where a `bw × bh` mark goes in its corner of a `w × h` picture: its top left.
fn corner_at(corner: Corner, w: f32, h: f32, bw: f32, bh: f32) -> (f32, f32) {
    let m = (h * 0.035).max(2.0);
    let x = match corner {
        Corner::TopLeft | Corner::BottomLeft => m,
        Corner::TopRight | Corner::BottomRight => w - m - bw,
    };
    let y = match corner {
        Corner::TopLeft | Corner::TopRight => m,
        Corner::BottomLeft | Corner::BottomRight => h - m - bh,
    };
    (x.max(0.0), y.max(0.0))
}

/// The timeline's watermark drawn over `out` (a frame of the video): its picture (read from
/// `store`) if it has one in the media bin, else its words.
pub fn draw(out: &mut Raster, timeline: &Timeline, store: &dyn MediaStore) {
    let mark = timeline.watermark.clone().checked();
    if !mark.shows() || out.width() < 16 || out.height() < 16 {
        return;
    }
    let Some(mut pix) = canvas::to_pixmap(out) else { return };
    let (w, h) = (pix.width() as f32, pix.height() as f32);
    let tall = (mark.size * h).max(6.0);
    let picture = mark
        .picture
        .filter(|id| timeline.media.iter().any(|m| m.id == *id && m.kind == MediaKind::Picture))
        .and_then(|id| store.picture(id, 0))
        .and_then(|p| canvas::to_pixmap(&p));
    if let Some(pic) = picture {
        if pic.width() == 0 || pic.height() == 0 {
            return;
        }
        // As tall as asked, no wider than half the video.
        let f = (tall / pic.height() as f32).min(w * 0.5 / pic.width() as f32);
        let (bw, bh) = (pic.width() as f32 * f, pic.height() as f32 * f);
        let (x, y) = corner_at(mark.corner, w, h, bw, bh);
        let paint = PixmapPaint { opacity: mark.opacity, quality: tiny_skia::FilterQuality::Bilinear, ..PixmapPaint::default() };
        pix.draw_pixmap(0, 0, pic.as_ref(), &paint, Transform::from_scale(f, f).post_translate(x, y), None);
    } else {
        let style = Style { font_size: tall * 0.8, bold: true, text_color: Color::WHITE, align: TextAlign::Left, ..Style::default() };
        let mut lay = text::layout(&mark.words, &style, Some(w * 0.9));
        lay.lines.truncate(1);
        lay.width = lay.lines.iter().map(|l| l.width).fold(0.0, f32::max);
        lay.height = lay.line_height * lay.lines.len() as f32;
        if lay.lines.is_empty() || lay.width <= 0.0 {
            return;
        }
        let (x, y) = corner_at(mark.corner, w, h, lay.width, lay.height);
        let Some(path) = text::text_path(&lay, &style, Pt::new(x, y), lay.width) else { return };
        let paint = |c: Color, a: f32| {
            let mut p = Paint::default();
            p.set_color(draw::skia_color(c, a));
            p.anti_alias = true;
            p
        };
        // A soft dark edge so it reads on any picture, then the white words.
        let edge = tiny_skia::Stroke { width: (tall * 0.08).max(1.0), line_join: tiny_skia::LineJoin::Round, ..tiny_skia::Stroke::default() };
        pix.stroke_path(&path, &paint(Color::BLACK, mark.opacity * 0.6), &edge, Transform::identity(), None);
        pix.fill_path(&path, &paint(Color::WHITE, mark.opacity), FillRule::Winding, Transform::identity(), None);
    }
    if let Some(r) = canvas::from_pixmap(&pix) {
        *out = r;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::layers::MediaItem;

    struct Logo;

    impl MediaStore for Logo {
        fn picture(&self, _id: u32, _t: u64) -> Option<Arc<Raster>> {
            Raster::filled(40, 20, Color::rgb(0, 200, 0)).ok().map(Arc::new)
        }
        fn sound(&self, _id: u32, _first: u64, count: usize) -> Vec<f32> {
            vec![0.0; count * 2]
        }
    }

    fn timeline(mark: Watermark) -> Timeline {
        let rec = {
            use crate::recording::{Header, RecordingWriter, StreamInfo, StreamKind};
            let streams = vec![StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 16, height: 16, channels: 0 }];
            let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
            w.video_frame(0, 0, &Raster::filled(16, 16, Color::BLACK).unwrap()).unwrap();
            crate::recording::Recording::open(Box::new(w.finish(100_000).unwrap())).unwrap()
        };
        let mut tl = Timeline::new(&rec);
        tl.media.push(MediaItem { id: 3, name: "Logo.png".into(), kind: MediaKind::Picture, width: 40, height: 20, ..MediaItem::default() });
        tl.watermark = mark;
        tl
    }

    /// How many pixels of each quarter of the picture aren't grey any more.
    fn marked(pic: &Raster) -> [usize; 4] {
        let (w, h) = (pic.width(), pic.height());
        let mut out = [0; 4];
        for y in 0..h {
            for x in 0..w {
                if pic.pixel(x, y).is_some_and(|p| p != Color::rgb(90, 90, 90)) {
                    out[usize::from(y >= h / 2) * 2 + usize::from(x >= w / 2)] += 1;
                }
            }
        }
        out
    }

    #[test]
    fn words_or_a_picture_go_in_their_corner_as_see_through_as_asked() {
        let grey = || Raster::filled(320, 180, Color::rgb(90, 90, 90)).unwrap();
        // Off, or on with nothing to show: nothing drawn.
        for mark in [Watermark { words: "Draft".into(), ..Watermark::default() }, Watermark { on: true, ..Watermark::default() }] {
            let mut pic = grey();
            draw(&mut pic, &timeline(mark), &Logo);
            assert_eq!(marked(&pic), [0; 4]);
        }
        // Words at the bottom right.
        let mut pic = grey();
        draw(&mut pic, &timeline(Watermark { on: true, words: "Draft".into(), ..Watermark::default() }), &Logo);
        let m = marked(&pic);
        assert!(m[3] > 50 && m[0] == 0 && m[1] == 0 && m[2] == 0, "{m:?}");
        // The bin's picture at the top left, half seen: green over grey, half way.
        let mut pic = grey();
        let mark = Watermark { on: true, words: "Draft".into(), picture: Some(3), corner: Corner::TopLeft, opacity: 0.5, size: 0.2 };
        draw(&mut pic, &timeline(mark.clone()), &Logo);
        let m = marked(&pic);
        assert!(m[0] > 500 && m[1] + m[2] + m[3] == 0, "{m:?}");
        let p = pic.pixel(20, 15).unwrap();
        assert!((p.g as i32 - 145).abs() < 6 && (p.r as i32 - 45).abs() < 6, "{p:?}");
        // A picture not in the bin: its words instead.
        let mut pic = grey();
        draw(&mut pic, &timeline(Watermark { picture: Some(9), corner: Corner::TopLeft, ..mark }), &Logo);
        assert!(marked(&pic)[0] > 50);
        // Hostile values are made sensible, never panicked over.
        let wild =
            Watermark { on: true, words: "x\u{0}\n".repeat(500), picture: None, corner: Corner::BottomLeft, opacity: f32::NAN, size: 1e30 }.checked();
        assert!(wild.words.chars().count() <= MAX_WORDS && wild.opacity == 0.6 && wild.size == MAX_SIZE);
        let mut tiny = Raster::filled(4, 4, Color::BLACK).unwrap();
        draw(&mut tiny, &timeline(wild), &Logo);
    }
}
