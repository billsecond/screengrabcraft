//! Scrolling capture: frames of one area taken while its content scrolls, joined into one tall
//! picture.
//!
//! Each new frame is compared with the one before by row fingerprints: rows of the new frame
//! that carry information (not one flat colour) are looked up among the previous frame's rows,
//! and the offset most of them agree on is how far the content scrolled. A frame is only
//! accepted when most of the overlap matches at that offset. Rows that stay put at the top or
//! bottom of the area (a toolbar, a sticky header, a status bar) are recognised and kept once.

use std::collections::HashMap;

use screengrabcraft_model::{MAX_PIXELS, MAX_SIDE, Raster};

/// What a new frame did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The first frame.
    Started,
    /// The content moved up by this many rows, and they were added.
    Scrolled(u32),
    /// Nothing moved.
    Unchanged,
    /// The frame could not be matched with the one before (it moved too far, or something other
    /// than scrolling changed); it was left out.
    Lost,
    /// The picture is as long as it can get; nothing more is added.
    Full,
    /// The frame is not the size of the others.
    WrongSize,
}

pub struct Stitcher {
    width: u32,
    height: u32,
    /// The picture so far, rows of `width` RGBA pixels.
    rows: Vec<u8>,
    total_rows: u32,
    last: Option<Raster>,
    last_hashes: Vec<u64>,
}

/// Columns at the right edge left out of the fingerprints (a scroll bar moves when scrolling).
const SCROLLBAR: u32 = 24;

fn row_hash(r: &Raster, y: u32) -> (u64, bool) {
    let w = r.width();
    let x1 = if w > SCROLLBAR * 3 { w - SCROLLBAR } else { w };
    let stride = w as usize * 4;
    let start = y as usize * stride;
    let Some(row) = r.pixels().get(start..start + x1 as usize * 4) else { return (0, false) };
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let first = row.get(..3);
    let mut flat = true;
    for px in row.as_chunks::<4>().0 {
        for b in &px[..3] {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        if flat && Some(&px[..3]) != first {
            flat = false;
        }
    }
    (h, !flat)
}

impl Stitcher {
    pub fn new() -> Stitcher {
        Stitcher { width: 0, height: 0, rows: Vec::new(), total_rows: 0, last: None, last_hashes: Vec::new() }
    }

    /// How long the picture is so far.
    pub fn length(&self) -> u32 {
        self.total_rows
    }

    pub fn push(&mut self, frame: Raster) -> Step {
        let hashes: Vec<(u64, bool)> = (0..frame.height()).map(|y| row_hash(&frame, y)).collect();
        if self.last.is_none() {
            self.width = frame.width();
            self.height = frame.height();
            self.rows = frame.pixels().to_vec();
            self.total_rows = frame.height();
            self.last_hashes = hashes.iter().map(|(h, _)| *h).collect();
            self.last = Some(frame);
            return Step::Started;
        }
        if frame.width() != self.width || frame.height() != self.height {
            return Step::WrongSize;
        }
        let h = self.height as usize;
        let prev_hashes = &self.last_hashes;
        if hashes.iter().zip(prev_hashes.iter()).all(|((a, _), b)| a == b) {
            return Step::Unchanged;
        }
        // Rows that stayed in place at the top and bottom.
        let header = hashes.iter().zip(prev_hashes.iter()).take_while(|((a, _), b)| a == *b).count();
        let footer = hashes.iter().rev().zip(prev_hashes.iter().rev()).take_while(|((a, _), b)| a == *b).count();
        if header + footer >= h {
            return Step::Unchanged;
        }
        let body = header..h - footer;
        // Where each fingerprint was in the previous frame's body.
        let mut where_prev: HashMap<u64, Vec<usize>> = HashMap::new();
        for y in body.clone() {
            if let Some(hv) = prev_hashes.get(y) {
                where_prev.entry(*hv).or_default().push(y);
            }
        }
        let mut votes: HashMap<usize, u32> = HashMap::new();
        for y in body.clone() {
            let Some((hv, informative)) = hashes.get(y) else { continue };
            if !informative {
                continue;
            }
            if let Some(ys) = where_prev.get(hv) {
                // A row seen many times (a repeated pattern) says little.
                if ys.len() > 4 {
                    continue;
                }
                for py in ys {
                    if *py > y {
                        *votes.entry(py - y).or_default() += 1;
                    }
                }
            }
        }
        let Some((&dy, &count)) = votes.iter().max_by_key(|(dy, n)| (**n, std::cmp::Reverse(**dy))) else { return Step::Lost };
        if count < 3 {
            return Step::Lost;
        }
        // Check the overlap at that offset.
        let (mut informative, mut matching) = (0u32, 0u32);
        for y in body.start..body.end.saturating_sub(dy) {
            if let (Some((hn, inf)), Some(hp)) = (hashes.get(y), prev_hashes.get(y + dy))
                && *inf
            {
                informative += 1;
                if hn == hp {
                    matching += 1;
                }
            }
        }
        if informative == 0 || matching * 10 < informative * 8 {
            return Step::Lost;
        }
        let dy32 = dy as u32;
        // The picture so far ends with the previous frame's bottom rows, the footer among them.
        let footer32 = (footer as u32).min(self.total_rows);
        let new_total = u64::from(self.total_rows) - u64::from(footer32) + u64::from(dy32) + u64::from(footer32);
        if new_total > u64::from(MAX_SIDE) || new_total * u64::from(self.width) > MAX_PIXELS {
            return Step::Full;
        }
        let stride = self.width as usize * 4;
        // Drop the footer, add the new rows and the footer again.
        let keep = (self.total_rows - footer32) as usize * stride;
        self.rows.truncate(keep);
        let from = (h - footer - dy) * stride;
        let to = h * stride;
        if let Some(fresh) = frame.pixels().get(from..to) {
            self.rows.extend_from_slice(fresh);
        }
        self.total_rows = (self.rows.len() / stride.max(1)) as u32;
        self.last_hashes = hashes.iter().map(|(h, _)| *h).collect();
        self.last = Some(frame);
        Step::Scrolled(dy32)
    }

    /// The picture so far.
    pub fn picture(&self) -> Option<Raster> {
        Raster::new(self.width, self.total_rows, self.rows.clone()).ok()
    }

    pub fn finish(self) -> Option<Raster> {
        Raster::new(self.width, self.total_rows, self.rows).ok()
    }
}

impl Default for Stitcher {
    fn default() -> Stitcher {
        Stitcher::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tall "page" whose rows all differ, with a fixed header and footer, seen through a
    /// window of `h` rows scrolled to `top`.
    fn page_row(y: u32, x: u32) -> [u8; 4] {
        let v = (y.wrapping_mul(2_654_435_761) >> 8) as u8;
        let w = ((y * 7 + x * 13) % 251) as u8;
        [v, w, (x % 17) as u8 * 9, 255]
    }

    fn view(top: u32, w: u32, h: u32, header: u32, footer: u32) -> Raster {
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let c = if y < header {
                    [10, 20, (x % 50) as u8 + 30, 255]
                } else if y >= h - footer {
                    [200, (x % 40) as u8, 90, 255]
                } else {
                    page_row(top + y, x)
                };
                px.extend_from_slice(&c);
            }
        }
        Raster::new(w, h, px).unwrap()
    }

    #[test]
    fn joins_scrolled_frames_keeping_header_and_footer_once() {
        let (w, h, header, footer) = (120, 80, 6, 5);
        let mut s = Stitcher::new();
        assert_eq!(s.push(view(0, w, h, header, footer)), Step::Started);
        assert_eq!(s.push(view(0, w, h, header, footer)), Step::Unchanged);
        assert_eq!(s.push(view(17, w, h, header, footer)), Step::Scrolled(17));
        assert_eq!(s.push(view(40, w, h, header, footer)), Step::Scrolled(23));
        assert_eq!(s.push(view(41, w, h, header, footer)), Step::Scrolled(1));
        let out = s.finish().unwrap();
        assert_eq!(out.height(), h + 41);
        // The page reads straight through, and the footer is at the very end.
        let body_rows = h - header - footer + 41;
        for y in 0..body_rows {
            let want = page_row(y + header, 3);
            let got = out.pixel(3, header + y).unwrap();
            assert_eq!([got.r, got.g, got.b, got.a], want, "row {y}");
        }
        let foot = out.pixel(3, out.height() - 1).unwrap();
        assert_eq!(foot.r, 200);
    }

    #[test]
    fn refuses_what_is_not_scrolling() {
        let (w, h) = (60, 40);
        let mut s = Stitcher::new();
        s.push(view(0, w, h, 0, 0));
        assert_eq!(s.push(view(1000, w, h, 0, 0)), Step::Lost, "too far: nothing in common");
        assert_eq!(s.push(view(0, w + 1, h, 0, 0)), Step::WrongSize);
        let flat = Raster::filled(w, h, screengrabcraft_model::Color::WHITE).unwrap();
        assert_eq!(s.push(flat), Step::Lost);
        assert_eq!(s.length(), h);
    }
}
