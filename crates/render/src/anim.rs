//! Animated GIFs: written from recorded frames, and played back.
//!
//! Frames come in as they are captured, each with the time since the one before. A frame that
//! is the same as the one before only makes that one last longer, so a still screen costs
//! nothing, and a frame holds only the part that changed, as GIF frames draw over the one before.
//! Each frame gets its own palette of 256 colours (NeuQuant, which keeps text and interface
//! colours well). That is slow on a big area, so it runs on threads of its own and never holds
//! up the capture: frames are put in order again as they are written.
//!
//! [`Player`] reads a GIF a frame at a time, so a long recording holds one frame in memory.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, ImageDecoder, Limits};
use screengrabcraft_model::{MAX_PIXELS, MAX_SIDE, Raster};

use crate::codec::MAX_FILE;

#[derive(Debug, thiserror::Error)]
pub enum GifError {
    #[error("a GIF can be at most 65,535 pixels a side")]
    TooLarge,
    #[error("the frames are not all the same size")]
    WrongSize,
    #[error("writing the GIF failed: {0}")]
    Write(String),
}

/// The most threads turning frames into GIF data, at half the machine's.
const MAX_ENCODERS: usize = 4;
/// The most frames waiting for an encoder, and the most memory they may hold. Past that a frame
/// is left out, so a long burst of motion on a slow machine costs smoothness, not memory.
const MAX_WAITING: usize = 256;
const MAX_WAITING_BYTES: usize = 512 * 1024 * 1024;

/// A frame for an encoder: the frame, what it draws over, and how long it stays up.
struct Job {
    order: u64,
    before: Option<Arc<Raster>>,
    frame: Arc<Raster>,
    hundredths: u16,
}

pub struct GifWriter<W> {
    jobs: SyncSender<Job>,
    encoders: Vec<JoinHandle<()>>,
    writing: JoinHandle<Result<(W, u32), GifError>>,
    width: u32,
    height: u32,
    /// The frame waiting to be sent on, and how long it has been shown so far (ms).
    pending: Option<(Arc<Raster>, u32)>,
    /// The last frame sent on: what the next one draws over.
    before: Option<Arc<Raster>>,
    order: u64,
    /// What a frame's time lost to GIF's hundredths of a second, given to the next one.
    owed_ms: u32,
}

impl<W: Write + Send + 'static> GifWriter<W> {
    pub fn new(out: W, width: u32, height: u32) -> Result<GifWriter<W>, GifError> {
        let frame_bytes = (width as usize).saturating_mul(height as usize).saturating_mul(4).max(1);
        let cores = std::thread::available_parallelism().map_or(2, std::num::NonZero::get);
        Self::with_threads(out, width, height, (MAX_WAITING_BYTES / frame_bytes).clamp(2, MAX_WAITING), (cores / 2).clamp(1, MAX_ENCODERS))
    }

    fn with_threads(out: W, width: u32, height: u32, waiting: usize, encoders: usize) -> Result<GifWriter<W>, GifError> {
        let (w, h) = (u16::try_from(width).map_err(|_| GifError::TooLarge)?, u16::try_from(height).map_err(|_| GifError::TooLarge)?);
        let mut encoder = gif::Encoder::new(out, w, h, &[]).map_err(write_error)?;
        encoder.set_repeat(gif::Repeat::Infinite).map_err(write_error)?;

        let (jobs, queue) = mpsc::sync_channel::<Job>(waiting);
        let queue = Arc::new(Mutex::new(queue));
        let (done, encoded) = mpsc::channel();
        let mut threads = Vec::with_capacity(encoders);
        for n in 0..encoders {
            let (queue, done) = (Arc::clone(&queue), done.clone());
            let thread = std::thread::Builder::new().name(format!("gif-encoder-{n}")).spawn(move || encode_jobs(&queue, &done));
            threads.push(thread.map_err(write_error)?);
        }
        let writing =
            std::thread::Builder::new().name("gif-writer".to_string()).spawn(move || write_in_order(encoder, &encoded)).map_err(write_error)?;
        Ok(GifWriter { jobs, encoders: threads, writing, width, height, pending: None, before: None, order: 0, owed_ms: 0 })
    }

    /// Adds a frame shown `delay_ms` after the previous one, waiting for room if it must.
    pub fn push(&mut self, frame: Raster, delay_ms: u32) -> Result<(), GifError> {
        self.add(frame, delay_ms, true).map(|_| ())
    }

    /// Adds a frame if there's room for it right now, for a recording that can't wait. False
    /// when it was left out: the next frame's delay then counts from the last one taken.
    pub fn offer(&mut self, frame: Raster, delay_ms: u32) -> Result<bool, GifError> {
        self.add(frame, delay_ms, false)
    }

    fn add(&mut self, frame: Raster, delay_ms: u32, wait: bool) -> Result<bool, GifError> {
        if frame.width() != self.width || frame.height() != self.height {
            return Err(GifError::WrongSize);
        }
        let Some((prev, shown)) = self.pending.take() else {
            self.pending = Some((Arc::new(frame), 0));
            return Ok(true);
        };
        if prev.pixels() == frame.pixels() {
            self.pending = Some((prev, shown.saturating_add(delay_ms)));
            return Ok(true);
        }
        // The previous frame lasts until this one comes.
        let (job, owed) = self.job(&prev, shown.saturating_add(delay_ms));
        let taken = if wait {
            self.jobs.send(job).map_err(|_| stopped())?;
            true
        } else {
            match self.jobs.try_send(job) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => false,
                Err(TrySendError::Disconnected(_)) => return Err(stopped()),
            }
        };
        if !taken {
            self.pending = Some((prev, shown));
            return Ok(false);
        }
        self.before = Some(prev);
        self.order += 1;
        self.owed_ms = owed;
        self.pending = Some((Arc::new(frame), 0));
        Ok(true)
    }

    /// The job for a frame shown for `shown_ms`, and the time that leaves owed to the next one.
    fn job(&self, frame: &Arc<Raster>, shown_ms: u32) -> (Job, u32) {
        let total = shown_ms.saturating_add(self.owed_ms);
        let hundredths = (total / 10).max(2);
        let job = Job {
            order: self.order,
            before: self.before.clone(),
            frame: Arc::clone(frame),
            hundredths: u16::try_from(hundredths).unwrap_or(u16::MAX),
        };
        (job, total.saturating_sub(hundredths.saturating_mul(10)))
    }

    /// Writes the last frame (shown for `last_ms`), waits for what's under way, and ends the file.
    pub fn finish(mut self, last_ms: u32) -> Result<(W, u32), GifError> {
        if let Some((prev, shown)) = self.pending.take() {
            let (job, _) = self.job(&prev, shown.saturating_add(last_ms));
            // If this fails the writing has stopped, and it says why below.
            let _ = self.jobs.send(job);
        }
        drop(self.jobs);
        for encoder in self.encoders {
            let _ = encoder.join();
        }
        self.writing.join().map_err(|_| stopped())?
    }
}

fn write_error(e: impl std::fmt::Display) -> GifError {
    GifError::Write(e.to_string())
}

fn stopped() -> GifError {
    GifError::Write("the GIF stopped being written".to_string())
}

/// An encoder's thread: jobs until there are no more.
fn encode_jobs(queue: &Mutex<Receiver<Job>>, done: &Sender<(u64, gif::Frame<'static>)>) {
    loop {
        let job = match queue.lock() {
            Ok(queue) => queue.recv(),
            Err(_) => return,
        };
        let Ok(job) = job else { return };
        if done.send((job.order, encode(&job))).is_err() {
            return;
        }
    }
}

/// The writer's thread: frames as they're ready, in the order they were taken.
fn write_in_order<W: Write>(mut encoder: gif::Encoder<W>, encoded: &Receiver<(u64, gif::Frame<'static>)>) -> Result<(W, u32), GifError> {
    let mut waiting = BTreeMap::new();
    let (mut next, mut frames) = (0, 0);
    for (order, frame) in encoded {
        waiting.insert(order, frame);
        while let Some(frame) = waiting.remove(&next) {
            encoder.write_lzw_pre_encoded_frame(&frame).map_err(write_error)?;
            next += 1;
            frames += 1;
        }
    }
    Ok((encoder.into_inner().map_err(write_error)?, frames))
}

/// A frame as GIF data: only what changed from the frame before.
fn encode(job: &Job) -> gif::Frame<'static> {
    let (x, y, w, h) =
        job.before.as_ref().and_then(|before| changed_area(before, &job.frame)).unwrap_or((0, 0, job.frame.width(), job.frame.height()));
    let part = job.frame.crop(x, y, w, h).unwrap_or_else(|| (*job.frame).clone());
    let (pw, ph) = (part.width(), part.height());
    // Big frames quantise faster (NeuQuant's speed: 1 is best and slowest, 30 fastest).
    let speed = if u64::from(pw) * u64::from(ph) > 1_000_000 { 20 } else { 10 };
    let side = |v: u32| u16::try_from(v).unwrap_or(u16::MAX);
    let mut rgba = part.into_pixels();
    let mut frame = gif::Frame::from_rgba_speed(side(pw), side(ph), &mut rgba, speed);
    frame.left = side(x);
    frame.top = side(y);
    frame.delay = job.hundredths;
    frame.dispose = gif::DisposalMethod::Keep;
    frame.make_lzw_pre_encoded();
    frame
}

/// The smallest area holding every pixel that differs, as x, y, width and height; none if none do.
fn changed_area(before: &Raster, frame: &Raster) -> Option<(u32, u32, u32, u32)> {
    let stride = (frame.width() as usize).saturating_mul(4);
    if stride == 0 || before.pixels().len() != frame.pixels().len() {
        return None;
    }
    let (mut top, mut bottom, mut left, mut right) = (None, 0, usize::MAX, 0);
    for (y, (a, b)) in before.pixels().chunks_exact(stride).zip(frame.pixels().chunks_exact(stride)).enumerate() {
        if a == b {
            continue;
        }
        top.get_or_insert(y);
        bottom = y;
        left = left.min(a.iter().zip(b).position(|(p, q)| p != q).unwrap_or(0) / 4);
        right = right.max(a.iter().zip(b).rposition(|(p, q)| p != q).unwrap_or(0) / 4);
    }
    let top = top?;
    let side = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
    Some((side(left), side(top), side(right.saturating_sub(left) + 1), side(bottom.saturating_sub(top) + 1)))
}

/// The most frames counted in a file (each costs a moment when it's opened).
const MAX_FRAMES: u32 = 200_000;
/// How long a frame with next to no delay of its own stays up, as browsers do.
const BARE_FRAME_MS: u32 = 100;

fn shown_ms(delay_ms: u32) -> u32 {
    if delay_ms < 20 { BARE_FRAME_MS } else { delay_ms }
}

/// One whole frame of a GIF and how long it stays up.
pub struct Frame {
    pub picture: Raster,
    pub delay_ms: u32,
}

/// A GIF played from its bytes. The bytes are hostile: the decoder has limits, and a file cut
/// short plays what it has.
pub struct Player {
    bytes: Arc<[u8]>,
    frames: image::Frames<'static>,
    size: (u32, u32),
    count: u32,
    total_ms: u64,
    elapsed_ms: u64,
}

impl Player {
    /// A player for an animated GIF; none for anything else (a still has nothing to play).
    pub fn open(bytes: Vec<u8>) -> Option<Player> {
        if !bytes.starts_with(b"GIF") || bytes.len() > MAX_FILE {
            return None;
        }
        let (count, total_ms) = measure(&bytes)?;
        if count < 2 {
            return None;
        }
        let bytes = Arc::<[u8]>::from(bytes);
        let (frames, size) = read(&bytes)?;
        Some(Player { bytes, frames, size, count, total_ms, elapsed_ms: 0 })
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// How many frames it has.
    pub fn count(&self) -> u32 {
        self.count
    }

    /// How long it runs, once through.
    pub fn total_ms(&self) -> u64 {
        self.total_ms
    }

    /// How far it has played, up to the end of the frame last read.
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }

    /// The next frame, from the first again after the last. None if no frame can be read.
    pub fn next_frame(&mut self) -> Option<Frame> {
        let frame = match self.frames.next() {
            Some(Ok(frame)) => frame,
            _ => {
                self.rewind();
                self.frames.next()?.ok()?
            }
        };
        let (numer, denom) = frame.delay().numer_denom_ms();
        let delay_ms = shown_ms(numer / denom.max(1));
        self.elapsed_ms = self.elapsed_ms.saturating_add(u64::from(delay_ms));
        let buffer = frame.into_buffer();
        let (w, h) = buffer.dimensions();
        let picture = Raster::new(w, h, buffer.into_raw()).ok()?;
        Some(Frame { picture, delay_ms })
    }

    /// Back to the first frame.
    pub fn rewind(&mut self) {
        self.elapsed_ms = 0;
        if let Some((frames, _)) = read(&self.bytes) {
            self.frames = frames;
        }
    }
}

/// How many frames a GIF has and how long it runs, without decoding them.
fn measure(bytes: &[u8]) -> Option<(u32, u64)> {
    let mut options = gif::DecodeOptions::new();
    options.skip_frame_decoding(true);
    let mut reader = options.read_info(Cursor::new(bytes)).ok()?;
    let (mut count, mut total_ms) = (0, 0u64);
    while count < MAX_FRAMES
        && let Ok(Some(frame)) = reader.next_frame_info()
    {
        count += 1;
        total_ms += u64::from(shown_ms(u32::from(frame.delay) * 10));
    }
    Some((count, total_ms))
}

/// A GIF's frames from the first, and its size.
fn read(bytes: &Arc<[u8]>) -> Option<(image::Frames<'static>, (u32, u32))> {
    let mut decoder = GifDecoder::new(Cursor::new(Arc::clone(bytes))).ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_PIXELS * 4 + 64 * 1024 * 1024);
    decoder.set_limits(limits).ok()?;
    let (w, h) = decoder.dimensions();
    screengrabcraft_model::raster::checked_len(w, h).ok()?;
    Some((decoder.into_frames(), (w, h)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Color;

    const BLUE: Color = Color::rgb(0, 0, 255);
    const GREEN: Color = Color::rgb(0, 255, 0);

    /// Three frames, 200 ms, 300 ms and 400 ms.
    fn three_frames() -> Vec<u8> {
        let mut w = GifWriter::new(Vec::new(), 8, 6).unwrap();
        w.push(Raster::filled(8, 6, Color::RED).unwrap(), 0).unwrap();
        w.push(Raster::filled(8, 6, BLUE).unwrap(), 200).unwrap();
        w.push(Raster::filled(8, 6, GREEN).unwrap(), 300).unwrap();
        w.finish(400).unwrap().0
    }

    fn first_pixel(frame: &Frame) -> [u8; 3] {
        [frame.picture.pixels()[0], frame.picture.pixels()[1], frame.picture.pixels()[2]]
    }

    /// Each frame's position, size and delay as written.
    fn frames_in(bytes: &[u8]) -> Vec<(u16, u16, u16, u16, u16)> {
        let mut options = gif::DecodeOptions::new();
        options.skip_frame_decoding(true);
        let mut dec = options.read_info(Cursor::new(bytes)).unwrap();
        let mut out = Vec::new();
        while let Some(f) = dec.next_frame_info().unwrap() {
            out.push((f.left, f.top, f.width, f.height, f.delay));
        }
        out
    }

    /// A picture no two frames share, with too many colours for a simple palette.
    fn noise(w: u32, h: u32, seed: u32) -> Raster {
        let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let [r, g, b, _] = state.to_le_bytes();
            px.extend_from_slice(&[r, g, b, 255]);
        }
        Raster::new(w, h, px).unwrap()
    }

    #[test]
    fn a_frame_holds_only_what_changed_and_plays_back_whole() {
        let first = Raster::filled(8, 6, Color::RED).unwrap();
        let mut second = first.clone();
        for (x, y) in [(2, 1), (4, 3)] {
            second.pixels_mut()[(y * 8 + x) * 4..(y * 8 + x) * 4 + 3].copy_from_slice(&[0, 0, 255]);
        }
        let mut w = GifWriter::new(Vec::new(), 8, 6).unwrap();
        w.push(first.clone(), 0).unwrap();
        w.push(second.clone(), 100).unwrap();
        let (bytes, frames) = w.finish(100).unwrap();
        assert_eq!(frames, 2);
        assert_eq!(frames_in(&bytes), vec![(0, 0, 8, 6, 10), (2, 1, 3, 3, 10)], "the second frame is just the box round the change");

        let mut p = Player::open(bytes).unwrap();
        assert_eq!(p.next_frame().unwrap().picture.pixels(), first.pixels());
        let back = p.next_frame().unwrap().picture;
        assert_eq!(back.pixels()[(8 + 2) * 4..(8 + 2) * 4 + 3], [0, 0, 255]);
        assert_eq!(back.pixels()[(3 * 8 + 4) * 4..(3 * 8 + 4) * 4 + 3], [0, 0, 255]);
        assert_eq!(back.pixels()[..3], first.pixels()[..3], "what didn't change is still there");
    }

    #[test]
    fn frames_stay_in_order_however_the_encoders_finish() {
        // A whole new picture, then one changed pixel: the small job often finishes first.
        let mut w = GifWriter::with_threads(Vec::new(), 300, 300, 8, 4).unwrap();
        let mut last = noise(300, 300, 0);
        for i in 0..24u32 {
            let frame = if i % 2 == 0 {
                noise(300, 300, i + 10)
            } else {
                let mut f = last.clone();
                f.pixels_mut()[0] ^= 0x80;
                f
            };
            last = frame.clone();
            w.push(frame, 100 + i * 10).unwrap();
        }
        let (bytes, frames) = w.finish(100).unwrap();
        assert_eq!(frames, 24);
        let delays: Vec<u16> = frames_in(&bytes).iter().map(|f| f.4).collect();
        let want: Vec<u16> = (1..24).map(|i| 10 + i).chain([10]).collect();
        assert_eq!(delays, want, "each frame's own delay, in the order they came");
    }

    #[test]
    fn no_time_is_lost_to_hundredths_of_a_second() {
        let mut w = GifWriter::new(Vec::new(), 4, 4).unwrap();
        for i in 0..31u32 {
            let mut f = Raster::filled(4, 4, Color::RED).unwrap();
            f.pixels_mut()[0] = i as u8;
            w.push(f, 33).unwrap();
        }
        let (bytes, _) = w.finish(33).unwrap();
        let total: u32 = frames_in(&bytes).iter().map(|f| u32::from(f.4) * 10).sum();
        assert!((30 * 33 + 33 - 10..=30 * 33 + 33).contains(&total), "{total} ms");
    }

    #[test]
    fn a_recording_leaves_frames_out_rather_than_wait_or_lose_time() {
        let frames: Vec<Raster> = (0..30).map(|i| noise(400, 400, i)).collect();
        let mut w = GifWriter::with_threads(Vec::new(), 400, 400, 1, 1).unwrap();
        let (mut owed, mut left_out) = (0, 0);
        for frame in frames {
            owed += 40;
            if w.offer(frame, owed).unwrap() {
                owed = 0;
            } else {
                left_out += 1;
            }
        }
        assert!(left_out > 0, "one slow encoder can't take them all");
        let (bytes, written) = w.finish(owed + 40).unwrap();
        assert_eq!(written, 30 - left_out);
        let total: u32 = frames_in(&bytes).iter().map(|f| u32::from(f.4) * 10).sum();
        assert!(total.abs_diff(30 * 40) <= 40, "the time is all there: {total} ms");
        assert!(Player::open(bytes).is_some());
    }

    #[test]
    fn writing_that_fails_is_reported() {
        struct Full;
        impl Write for Full {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("the disk is full"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        // The header may fail straight away, or the frames later: either way it's an error.
        let Ok(mut w) = GifWriter::new(Full, 4, 4) else { return };
        let _ = w.push(Raster::filled(4, 4, Color::RED).unwrap(), 0);
        let _ = w.push(Raster::filled(4, 4, BLUE).unwrap(), 100);
        assert!(w.finish(100).is_err());
    }

    #[test]
    fn plays_the_frames_in_order_and_goes_round_again() {
        let mut p = Player::open(three_frames()).unwrap();
        assert_eq!((p.size(), p.count(), p.total_ms()), ((8, 6), 3, 900));
        let reds = |f: &Frame| first_pixel(f)[0] > 200;
        let blues = |f: &Frame| first_pixel(f)[2] > 200;
        let greens = |f: &Frame| first_pixel(f)[1] > 200 && first_pixel(f)[0] < 100;

        let f = p.next_frame().unwrap();
        assert!(reds(&f), "{:?}", first_pixel(&f));
        assert_eq!((f.delay_ms, p.elapsed_ms()), (200, 200));
        let f = p.next_frame().unwrap();
        assert!(blues(&f), "{:?}", first_pixel(&f));
        assert_eq!((f.delay_ms, p.elapsed_ms()), (300, 500));
        let f = p.next_frame().unwrap();
        assert!(greens(&f), "{:?}", first_pixel(&f));
        assert_eq!((f.delay_ms, p.elapsed_ms()), (400, 900));
        let f = p.next_frame().unwrap();
        assert!(reds(&f), "back to the first: {:?}", first_pixel(&f));
        assert_eq!(p.elapsed_ms(), 200, "the clock starts over too");
    }

    #[test]
    fn rewinding_starts_from_the_first_frame() {
        let mut p = Player::open(three_frames()).unwrap();
        p.next_frame().unwrap();
        p.next_frame().unwrap();
        p.rewind();
        assert_eq!(p.elapsed_ms(), 0);
        assert_eq!(p.next_frame().unwrap().delay_ms, 200);
    }

    #[test]
    fn a_frame_with_no_delay_of_its_own_still_stays_up() {
        let mut bytes = Vec::new();
        {
            let mut enc = gif::Encoder::new(&mut bytes, 2, 2, &[]).unwrap();
            for _ in 0..2 {
                let mut rgba = vec![9; 16];
                enc.write_frame(&gif::Frame::from_rgba_speed(2, 2, &mut rgba, 30)).unwrap();
            }
        }
        let mut p = Player::open(bytes).unwrap();
        assert_eq!(p.total_ms(), u64::from(BARE_FRAME_MS) * 2);
        assert_eq!(p.next_frame().unwrap().delay_ms, BARE_FRAME_MS);
    }

    #[test]
    fn stills_and_other_pictures_have_nothing_to_play() {
        let red = Raster::filled(4, 4, Color::RED).unwrap();
        assert!(Player::open(crate::encode(&red, screengrabcraft_model::ImageFormat::Gif, 90).unwrap()).is_none());
        assert!(Player::open(crate::encode(&red, screengrabcraft_model::ImageFormat::Png, 90).unwrap()).is_none());
        assert!(Player::open(Vec::new()).is_none());
    }

    #[test]
    fn a_broken_gif_plays_what_it_has_and_never_panics() {
        let whole = three_frames();
        for end in 0..whole.len() {
            let mut cut = whole.clone();
            cut.truncate(end);
            if let Some(mut p) = Player::open(cut) {
                for _ in 0..8 {
                    let _ = p.next_frame();
                }
            }
        }
        for at in 0..whole.len() {
            let mut bad = whole.clone();
            bad[at] ^= 0xFF;
            if let Some(mut p) = Player::open(bad) {
                for _ in 0..8 {
                    let _ = p.next_frame();
                }
            }
        }
    }

    #[test]
    fn writes_frames_and_merges_still_ones() {
        let mut w = GifWriter::new(Vec::new(), 8, 6).unwrap();
        let red = Raster::filled(8, 6, Color::RED).unwrap();
        let blue = Raster::filled(8, 6, Color::rgb(0, 0, 255)).unwrap();
        w.push(red.clone(), 0).unwrap();
        w.push(red, 100).unwrap();
        w.push(blue.clone(), 100).unwrap();
        assert!(matches!(w.push(Raster::filled(2, 2, Color::RED).unwrap(), 100), Err(GifError::WrongSize)));
        let (bytes, frames) = w.finish(500).unwrap();
        assert_eq!(frames, 2, "the repeated red frame was merged");
        assert!(bytes.starts_with(b"GIF89a"));
        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::RGBA);
        let mut dec = opts.read_info(std::io::Cursor::new(bytes)).unwrap();
        let first = dec.read_next_frame().unwrap().unwrap();
        assert_eq!(first.delay, 20, "200 ms for the merged red");
        assert_eq!(&first.buffer[..3], &[0xE5, 0x39, 0x35]);
        assert!(GifWriter::new(Vec::new(), 70_000, 10).is_err());
    }
}
