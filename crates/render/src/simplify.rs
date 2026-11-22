//! Simplify: the parts of a picture of a screen found as plain shapes. Areas of one colour (a
//! panel, a bar, a button) become flat boxes in their main colour; lines of text become grey bars;
//! small marks (icons) become small boxes in their colour. The text's lines come from the reader
//! where it was read (Grab Text's line boxes), else from the marks themselves.
//!
//! It is worked out on a copy at most [`WORK_SIDE`] pixels a side: neighbouring pixels close in
//! colour are joined into areas, the largest is the background, large areas that fill most of
//! the box round them are blocks, and the small ones are marks, joined along a line into words
//! and lines of text.

use screengrabcraft_model::{Color, Raster, RasterError, Rect};

use crate::effects;

/// The longest side the picture is looked at in.
pub const WORK_SIDE: u32 = 800;
/// The most parts found (the largest are kept).
pub const MAX_PARTS: usize = 600;

/// What a part is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    /// An area of one colour: a panel, a bar, a button.
    Block,
    /// A line of text.
    Text,
    /// A small mark that isn't text: an icon.
    Mark,
}

/// A part of the picture, as a plain shape: its box (in the picture's pixels) and its colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub kind: PartKind,
    pub rect: Rect,
    pub colour: Color,
}

/// A picture simplified: the colour of its background and its parts, the ones under first.
#[derive(Clone, Debug, PartialEq)]
pub struct Simplified {
    pub background: Color,
    pub parts: Vec<Part>,
}

/// Areas of pixels joined by colour.
struct Areas {
    parent: Vec<u32>,
    size: Vec<u32>,
}

impl Areas {
    fn new(n: usize) -> Areas {
        Areas { parent: (0..n as u32).collect(), size: vec![1; n] }
    }

    fn root(&mut self, mut i: u32) -> u32 {
        for _ in 0..64 {
            let p = self.parent.get(i as usize).copied().unwrap_or(i);
            if p == i {
                return i;
            }
            let gp = self.parent.get(p as usize).copied().unwrap_or(p);
            if let Some(slot) = self.parent.get_mut(i as usize) {
                *slot = gp;
            }
            i = gp;
        }
        i
    }

    fn join(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.root(a), self.root(b));
        if ra == rb {
            return;
        }
        let (sa, sb) = (self.size.get(ra as usize).copied().unwrap_or(1), self.size.get(rb as usize).copied().unwrap_or(1));
        let (big, small) = if sa >= sb { (ra, rb) } else { (rb, ra) };
        if let Some(p) = self.parent.get_mut(small as usize) {
            *p = big;
        }
        if let Some(s) = self.size.get_mut(big as usize) {
            *s = sa.saturating_add(sb);
        }
    }
}

/// What is known of an area: how many pixels, the box round them, and their colours added up.
#[derive(Clone, Copy, Default)]
struct Stats {
    count: u32,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    sum: [u64; 4],
}

impl Stats {
    fn add(&mut self, x: u32, y: u32, px: &[u8]) {
        if self.count == 0 {
            (self.x0, self.y0, self.x1, self.y1) = (x, y, x, y);
        } else {
            self.x0 = self.x0.min(x);
            self.y0 = self.y0.min(y);
            self.x1 = self.x1.max(x);
            self.y1 = self.y1.max(y);
        }
        self.count += 1;
        for (s, c) in self.sum.iter_mut().zip(px) {
            *s += u64::from(*c);
        }
    }

    fn w(&self) -> u32 {
        self.x1 - self.x0 + 1
    }

    fn h(&self) -> u32 {
        self.y1 - self.y0 + 1
    }

    fn colour(&self) -> Color {
        let n = u64::from(self.count.max(1));
        let c = |k: usize| (self.sum.get(k).copied().unwrap_or(0) / n).min(255) as u8;
        Color::rgba(c(0), c(1), c(2), c(3))
    }

    fn rect(&self, f: f32) -> Rect {
        Rect::new(self.x0 as f32 / f, self.y0 as f32 / f, self.w() as f32 / f, self.h() as f32 / f)
    }
}

fn close(a: &[u8], b: &[u8], tolerance: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(*y) <= tolerance)
}

/// The grey a line of text is drawn as over `under`: darker on a light part, lighter on a dark one.
pub fn bar_grey(under: Color) -> Color {
    let lum = 0.299 * f32::from(under.r) + 0.587 * f32::from(under.g) + 0.114 * f32::from(under.b);
    if lum > 140.0 { Color::rgb(0xA4, 0xA9, 0xB0) } else { Color::rgb(0x70, 0x76, 0x7E) }
}

/// The parts of `picture`: `lines` are the boxes of its lines of text where they were read (in
/// its pixels), else none; `tolerance` is how far apart (0 to 255 a channel) neighbouring pixels
/// of one area may be.
pub fn simplify(picture: &Raster, lines: &[Rect], tolerance: u8) -> Result<Simplified, RasterError> {
    let (w0, h0) = (picture.width().max(1), picture.height().max(1));
    let f = (WORK_SIDE as f32 / w0.max(h0) as f32).min(1.0);
    let (sw, sh) = (((w0 as f32 * f).round() as u32).max(1), ((h0 as f32 * f).round() as u32).max(1));
    let small = effects::resize(picture, sw, sh)?;
    let px = small.pixels();
    let n = (sw as usize) * (sh as usize);
    let at = |i: usize| px.get(i * 4..i * 4 + 4).unwrap_or(&[0, 0, 0, 0]);

    // Neighbours close in colour, joined.
    let mut areas = Areas::new(n);
    for y in 0..sh as usize {
        for x in 0..sw as usize {
            let i = y * sw as usize + x;
            if x + 1 < sw as usize && close(at(i), at(i + 1), tolerance) {
                areas.join(i as u32, (i + 1) as u32);
            }
            if y + 1 < sh as usize && close(at(i), at(i + sw as usize), tolerance) {
                areas.join(i as u32, (i + sw as usize) as u32);
            }
        }
    }
    let mut label = vec![0u32; n];
    let mut stats: std::collections::HashMap<u32, Stats> = std::collections::HashMap::new();
    for (i, l) in label.iter_mut().enumerate() {
        let r = areas.root(i as u32);
        *l = r;
        stats.entry(r).or_default().add((i % sw as usize) as u32, (i / sw as usize) as u32, at(i));
    }
    let background = stats.iter().max_by_key(|(r, s)| (s.count, std::cmp::Reverse(**r))).map(|(r, _)| *r).unwrap_or(0);
    let bg = stats.get(&background).map(Stats::colour).unwrap_or(Color::WHITE);

    // Blocks: large areas that fill most of their box, other than the background.
    let min_block = (n as u32 / 1500).max(40);
    let mut blocks: Vec<(u32, Stats)> = stats
        .iter()
        .filter(|(r, s)| {
            let fill = s.count as f32 / (s.w() as f32 * s.h() as f32);
            **r != background && s.count >= min_block && s.w() >= 4 && s.h() >= 4 && fill >= 0.45 && s.colour().a > 16
        })
        .map(|(r, s)| (*r, *s))
        .collect();
    blocks.sort_by_key(|(r, s)| (std::cmp::Reverse(u64::from(s.w()) * u64::from(s.h())), *r));
    blocks.truncate(MAX_PARTS);
    let big = |r: u32| r == background || stats.get(&r).is_some_and(|s| s.count >= min_block);

    // Marks: the pixels of the small areas, joined along a row across small gaps (letters into
    // words and lines) and to the row below where they touch.
    let gap = ((6.0 * f).round() as usize).max(2);
    let mut marks = Areas::new(n);
    let ink = |i: usize| label.get(i).is_some_and(|l| !big(*l));
    for y in 0..sh as usize {
        let mut last: Option<usize> = None;
        for x in 0..sw as usize {
            let i = y * sw as usize + x;
            if !ink(i) {
                continue;
            }
            if let Some(l) = last
                && x - l <= gap + 1
            {
                marks.join((y * sw as usize + l) as u32, i as u32);
            }
            last = Some(x);
            if y > 0 && ink(i - sw as usize) {
                marks.join((i - sw as usize) as u32, i as u32);
            }
        }
    }
    let mut groups: std::collections::HashMap<u32, Stats> = std::collections::HashMap::new();
    for i in (0..n).filter(|&i| ink(i)) {
        let r = marks.root(i as u32);
        groups.entry(r).or_default().add((i % sw as usize) as u32, (i / sw as usize) as u32, at(i));
    }

    // What is under a point: the smallest block round it, else the background.
    let under = |x: f32, y: f32| -> Color {
        blocks
            .iter()
            .rev()
            .find(|(_, s)| x >= s.x0 as f32 && x <= s.x1 as f32 && y >= s.y0 as f32 && y <= s.y1 as f32)
            .map(|(_, s)| s.colour())
            .unwrap_or(bg)
    };
    let read: Vec<Rect> = lines.iter().map(|r| r.finite()).filter(|r| r.w > 0.0 && r.h > 0.0).collect();
    let in_read = |r: Rect| read.iter().any(|l| l.inflate(2.0).contains(r.center()));
    let mut texts = Vec::new();
    let mut icons = Vec::new();
    for s in groups.values() {
        if s.count < 4 || (s.w() < 2 && s.h() < 2) {
            continue;
        }
        let rect = s.rect(f);
        if in_read(rect) {
            continue;
        }
        let tall = (s.h() as f32 / f) <= 48.0;
        if tall && s.w() as f32 >= 1.8 * s.h() as f32 && s.h() >= 2 {
            let c = (s.x0 + s.x1) as f32 / 2.0;
            let m = (s.y0 + s.y1) as f32 / 2.0;
            texts.push(bar(rect, bar_grey(under(c, m))));
        } else {
            icons.push(Part { kind: PartKind::Mark, rect, colour: s.colour() });
        }
    }
    for l in &read {
        let c = l.center();
        texts.push(bar(*l, bar_grey(under(c.x * f, c.y * f))));
    }
    icons.sort_by(|a, b| (b.rect.w * b.rect.h).total_cmp(&(a.rect.w * a.rect.h)));
    let mut parts: Vec<Part> = blocks.iter().map(|(_, s)| Part { kind: PartKind::Block, rect: s.rect(f), colour: s.colour() }).collect();
    let room = MAX_PARTS.saturating_sub(parts.len());
    texts.sort_by(|a, b| (a.rect.y, a.rect.x).partial_cmp(&(b.rect.y, b.rect.x)).unwrap_or(std::cmp::Ordering::Equal));
    texts.truncate(room);
    icons.truncate(room.saturating_sub(texts.len()));
    parts.extend(icons);
    parts.extend(texts);
    Ok(Simplified { background: bg, parts })
}

/// A line of text's bar: as long as the line, a little more than half as tall, in the middle.
fn bar(line: Rect, colour: Color) -> Part {
    let h = (line.h * 0.6).max(2.0).min(line.h.max(2.0));
    Part { kind: PartKind::Text, rect: Rect::new(line.x, line.y + (line.h - h) / 2.0, line.w, h), colour }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up window: a light background, a dark title bar with a light mark in it, a blue
    /// button, and two lines of "text" (dark dashes with gaps, like letters).
    fn window() -> Raster {
        let mut r = Raster::filled(400, 300, Color::rgb(240, 240, 240)).unwrap();
        let mut fill = |x0: u32, y0: u32, w: u32, h: u32, c: Color| {
            let width = r.width() as usize;
            let px = r.pixels_mut();
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    let i = (y as usize * width + x as usize) * 4;
                    px[i..i + 4].copy_from_slice(&[c.r, c.g, c.b, 255]);
                }
            }
        };
        fill(0, 0, 400, 40, Color::rgb(40, 44, 52));
        fill(10, 12, 16, 16, Color::rgb(250, 200, 0));
        fill(260, 230, 120, 40, Color::rgb(30, 120, 230));
        for (y, words) in [(80u32, 6u32), (110, 4)] {
            for k in 0..words * 5 {
                // Letters three pixels wide, a pixel apart, a gap of four between words.
                let x = 20 + k * 4 + (k / 5) * 4;
                fill(x, y, 3, 12, Color::rgb(20, 20, 20));
            }
        }
        r
    }

    #[test]
    fn blocks_lines_of_text_and_marks_are_found() {
        let s = simplify(&window(), &[], 12).unwrap();
        assert_eq!(s.background, Color::rgb(240, 240, 240));
        let blocks: Vec<&Part> = s.parts.iter().filter(|p| p.kind == PartKind::Block).collect();
        let bar_found = |c: Color, r: Rect| blocks.iter().any(|p| p.colour == c && (p.rect.x - r.x).abs() <= 1.0 && (p.rect.w - r.w).abs() <= 1.0);
        assert!(bar_found(Color::rgb(40, 44, 52), Rect::new(0.0, 0.0, 400.0, 40.0)), "{blocks:?}");
        assert!(bar_found(Color::rgb(30, 120, 230), Rect::new(260.0, 230.0, 120.0, 40.0)), "{blocks:?}");
        // The two lines of text, each a bar as long as it.
        let texts: Vec<&Part> = s.parts.iter().filter(|p| p.kind == PartKind::Text).collect();
        assert_eq!(texts.len(), 2, "{texts:?}");
        let first = texts[0].rect;
        assert!((first.x - 20.0).abs() <= 1.0 && first.w > 120.0 && first.y > 80.0 && first.bottom() < 92.0, "{first:?}");
        assert_eq!(texts[0].colour, bar_grey(Color::rgb(240, 240, 240)));
        // The light mark in the title bar is a small box in its colour (it's a block of its own
        // at this size, or a mark).
        assert!(s.parts.iter().any(|p| p.colour == Color::rgb(250, 200, 0)), "{:?}", s.parts);
        // Under first: the blocks before the text.
        let last_block = s.parts.iter().rposition(|p| p.kind == PartKind::Block).unwrap();
        let first_text = s.parts.iter().position(|p| p.kind == PartKind::Text).unwrap();
        assert!(last_block < first_text);
    }

    #[test]
    fn lines_read_are_used_for_the_text() {
        let read = [Rect::new(18.0, 78.0, 140.0, 16.0)];
        let s = simplify(&window(), &read, 12).unwrap();
        let texts: Vec<&Part> = s.parts.iter().filter(|p| p.kind == PartKind::Text).collect();
        // The line read, and the other found from its marks.
        assert_eq!(texts.len(), 2, "{texts:?}");
        assert!(texts.iter().any(|p| (p.rect.x - 18.0).abs() < 0.01 && (p.rect.w - 140.0).abs() < 0.01));
    }

    #[test]
    fn a_large_picture_is_looked_at_smaller_and_answered_in_its_own_pixels() {
        let big = effects::resize(&window(), 1600, 1200).unwrap();
        let s = simplify(&big, &[], 12).unwrap();
        assert!(s.parts.iter().any(|p| p.kind == PartKind::Block && (p.rect.w - 1600.0).abs() < 6.0), "{:?}", s.parts);
    }

    #[test]
    fn odd_pictures_never_panic() {
        for (w, h) in [(1, 1), (1, 500), (3, 2)] {
            let r = Raster::filled(w, h, Color::TRANSPARENT).unwrap();
            assert!(simplify(&r, &[Rect::new(f32::NAN, 0.0, -1.0, 1e30)], 0).is_ok());
        }
        // Noise: every pixel different, parts capped.
        let mut r = Raster::filled(300, 300, Color::WHITE).unwrap();
        for (i, p) in r.pixels_mut().iter_mut().enumerate() {
            *p = (i.wrapping_mul(2_654_435_761) >> 7) as u8;
        }
        let s = simplify(&r, &[], 4).unwrap();
        assert!(s.parts.len() <= MAX_PARTS);
    }
}
