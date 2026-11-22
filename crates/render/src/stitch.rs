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

/// One notch of a mouse wheel, as systems count it.
pub const NOTCH: i32 = 120;
/// Frames in a row that didn't move, after the wheel was turned, before the page is taken to be at
/// its end. The wheel is turned again every [`RETRY_EVERY`] of them (the page may be slow, or a
/// turn too small to move it).
pub const QUIET_END: u32 = 6;
pub const RETRY_EVERY: u32 = 2;
/// Frames in a row that could not be joined before scrolling by itself gives up (something other
/// than scrolling keeps changing there).
pub const LOST_END: u32 = 6;
/// The smallest turn it steps down to when a turn went too far.
pub const MIN_TURN: i32 = 40;

/// Scrolling by itself: what to do after each frame, from what the frame did. The wheel is
/// turned a notch whenever the picture moved; the page has reached its end when the picture stops
/// changing; a frame that could not be joined (the turn went further than the area is tall) is
/// undone with the opposite turn and the turns are made smaller. A page that never moves when the
/// wheel turns is left to be scrolled by hand. It does no input itself: the service turns the
/// wheel and takes the frames, and the tests play a page made up.
#[derive(Clone, Debug)]
pub struct Pacer {
    /// The turn for one step.
    turn: i32,
    /// The turn sent last, undone when its frame is lost (0: nothing to undo).
    last: i32,
    /// Frames in a row that didn't move.
    quiet: u32,
    /// Frames in a row that could not be joined.
    lost: u32,
    /// Whether the page has moved at all since it began.
    moved: bool,
    /// Left to the owner: the page didn't move when the wheel turned.
    by_hand: bool,
    /// The longest the picture may get.
    cap: u32,
}

/// What to do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// Turn the wheel this much: more than 0 goes down the page, [`NOTCH`] is one notch.
    Wheel(i32),
    /// Take the next frame.
    Wait,
    /// The wheel moves nothing there: the owner scrolls it by hand from now on.
    ByHand,
    /// The picture is done.
    Stop(End),
}

/// Why scrolling by itself stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// The page stopped moving: its end.
    Bottom,
    /// The picture is as long as it may get.
    Long,
    /// The frames could not be joined again.
    Lost,
}

impl Pacer {
    /// Scrolls until the picture is `cap` rows long at most.
    pub fn new(cap: u32) -> Pacer {
        Pacer { turn: NOTCH, last: 0, quiet: 0, lost: 0, moved: false, by_hand: false, cap: cap.max(1) }
    }

    /// Whether the owner scrolls from now on.
    pub fn by_hand(&self) -> bool {
        self.by_hand
    }

    /// Leaves the scrolling to the owner from now on (the wheel can't be turned there): it only
    /// waits for frames, and still stops at the longest picture.
    pub fn hand_over(&mut self) {
        self.by_hand = true;
    }

    fn send(&mut self, turn: i32) -> Next {
        self.last = turn;
        Next::Wheel(turn)
    }

    /// What to do after a frame that did `step`, the picture now `length` rows long.
    pub fn after(&mut self, step: Step, length: u32) -> Next {
        if matches!(step, Step::Full) || length >= self.cap {
            return Next::Stop(End::Long);
        }
        if self.by_hand {
            return Next::Wait;
        }
        match step {
            Step::Started | Step::Scrolled(_) => {
                self.moved |= step != Step::Started;
                self.quiet = 0;
                self.lost = 0;
                self.send(self.turn)
            }
            Step::Unchanged => {
                self.quiet = self.quiet.saturating_add(1);
                if self.quiet >= QUIET_END {
                    if self.moved {
                        return Next::Stop(End::Bottom);
                    }
                    self.by_hand = true;
                    return Next::ByHand;
                }
                if self.quiet.is_multiple_of(RETRY_EVERY) { self.send(self.turn) } else { Next::Wait }
            }
            Step::Lost => {
                self.quiet = 0;
                self.lost = self.lost.saturating_add(1);
                if self.lost >= LOST_END {
                    return Next::Stop(End::Lost);
                }
                // Lost again before the undo shows: nothing more to undo, and the turns stay as
                // they are.
                let undo = self.last.saturating_neg();
                self.last = 0;
                if undo == 0 {
                    return Next::Wait;
                }
                self.turn = (self.turn / 2).max(MIN_TURN);
                Next::Wheel(undo)
            }
            Step::WrongSize | Step::Full => Next::Wait,
        }
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

    /// Scrolling by itself, played on a made-up page: its top can go from 0 to `end`, it's seen
    /// through an area `h` rows tall (a header of 6 and a footer of 5 staying put), a notch of the
    /// wheel moves it `per_notch` rows, and a turn shows `lag` frames after it's made. The
    /// picture, how it ended and the turns made.
    fn play(end: u32, h: u32, per_notch: i32, lag: usize, cap: u32) -> (Raster, Next, Vec<i32>) {
        let (w, header, footer) = (90, 6, 5);
        let mut s = Stitcher::new();
        let mut pacer = Pacer::new(cap);
        let mut top = 0i64;
        let mut coming = std::collections::VecDeque::new();
        let mut turns = Vec::new();
        for _ in 0..5000 {
            while coming.len() > lag {
                let turn: i32 = coming.pop_front().unwrap();
                top = (top + i64::from(turn) * i64::from(per_notch) / i64::from(NOTCH)).clamp(0, i64::from(end));
            }
            let step = s.push(view(top as u32, w, h, header, footer));
            match pacer.after(step, s.length()) {
                Next::Wheel(turn) => {
                    turns.push(turn);
                    coming.push_back(turn);
                }
                Next::Wait => coming.push_back(0),
                last @ (Next::Stop(_) | Next::ByHand) => return (s.finish().unwrap(), last, turns),
            }
        }
        panic!("it never stopped");
    }

    /// The page reads straight through the picture, from its top to `rows` rows of it.
    fn reads_straight(out: &Raster, rows: u32) {
        for y in 0..rows {
            let want = page_row(y + 6, 3);
            let got = out.pixel(3, 6 + y).unwrap();
            assert_eq!([got.r, got.g, got.b, got.a], want, "row {y}");
        }
        assert_eq!(out.pixel(3, out.height() - 1).unwrap().r, 200, "the footer is at the very end");
    }

    #[test]
    fn scrolling_by_itself_goes_to_the_end_of_the_page_and_stops() {
        for lag in [0, 1, 2] {
            let (out, end, turns) = play(500, 80, 40, lag, 60_000);
            assert_eq!(end, Next::Stop(End::Bottom), "lag {lag}");
            assert_eq!(out.height(), 80 + 500, "the whole page, lag {lag}");
            reads_straight(&out, 80 + 500 - 11);
            assert!(turns.iter().all(|t| *t == NOTCH), "a notch at a time: {turns:?}");
        }
    }

    #[test]
    fn a_turn_that_goes_too_far_is_undone_and_the_turns_get_smaller() {
        // A notch moves the page 100 rows: more than the 69 that change in the area.
        let (out, end, turns) = play(400, 80, 100, 1, 60_000);
        assert_eq!(end, Next::Stop(End::Bottom));
        assert_eq!(out.height(), 80 + 400);
        reads_straight(&out, 80 + 400 - 11);
        assert_eq!(turns.get(..2), Some(&[NOTCH, -NOTCH][..]), "the first turn is taken back");
        assert!(turns.iter().skip(2).all(|t| *t == NOTCH / 2), "then half a notch: {turns:?}");
    }

    #[test]
    fn scrolling_by_itself_stops_at_the_longest_picture() {
        let (out, end, _) = play(5000, 80, 40, 0, 300);
        assert_eq!(end, Next::Stop(End::Long));
        assert!((300..300 + 40).contains(&out.height()), "{}", out.height());
        reads_straight(&out, out.height() - 11);
    }

    #[test]
    fn a_page_the_wheel_doesnt_move_is_left_to_be_scrolled_by_hand() {
        let (out, end, turns) = play(500, 80, 0, 0, 60_000);
        assert_eq!(end, Next::ByHand);
        assert_eq!(out.height(), 80);
        assert_eq!(turns.len() as u32, 1 + (QUIET_END - 1) / RETRY_EVERY, "tried a few times: {turns:?}");
        // From then on it only waits for the owner, and still stops at the longest picture.
        let mut p = Pacer::new(200);
        for _ in 0..QUIET_END {
            p.after(Step::Unchanged, 80);
        }
        assert!(p.by_hand());
        assert_eq!(p.after(Step::Scrolled(40), 120), Next::Wait);
        assert_eq!(p.after(Step::Unchanged, 120), Next::Wait);
        assert_eq!(p.after(Step::Scrolled(90), 210), Next::Stop(End::Long));
    }

    #[test]
    fn scrolling_by_itself_gives_up_on_an_area_that_changes_of_itself() {
        let (w, h) = (60, 40);
        let mut s = Stitcher::new();
        let mut p = Pacer::new(60_000);
        assert_eq!(p.after(s.push(view(0, w, h, 0, 0)), s.length()), Next::Wheel(NOTCH));
        let mut last = Next::Wait;
        for i in 1..=LOST_END {
            last = p.after(s.push(view(i * 5000, w, h, 0, 0)), s.length());
            if i == 1 {
                assert_eq!(last, Next::Wheel(-NOTCH), "the turn is taken back");
            } else if i < LOST_END {
                assert_eq!(last, Next::Wait, "nothing more to take back");
            }
        }
        assert_eq!(last, Next::Stop(End::Lost));
        assert_eq!(s.length(), h, "what it had is kept");
    }

    #[test]
    fn the_pacer_takes_any_steps_without_fuss() {
        let steps = [Step::Started, Step::Scrolled(0), Step::Scrolled(u32::MAX), Step::Unchanged, Step::Lost, Step::WrongSize, Step::Full];
        for cap in [0, 1, u32::MAX] {
            let mut p = Pacer::new(cap);
            for i in 0..500usize {
                let step = steps[(i * 7 + i / 3) % steps.len()];
                let _ = p.after(step, (i as u32).wrapping_mul(2_654_435_761));
            }
        }
    }
}
