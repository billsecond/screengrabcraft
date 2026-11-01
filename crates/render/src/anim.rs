//! Animated GIFs: written from recorded frames, and played back.
//!
//! Frames come in as they are captured, each with the time since the one before. A frame that
//! is the same as the one before only makes that one last longer, so a still screen costs
//! nothing. Each frame gets its own palette of 256 colours (NeuQuant, which keeps text and
//! interface colours well).
//!
//! [`Player`] reads a GIF a frame at a time, so a long recording holds one frame in memory.

use std::io::{Cursor, Write};
use std::sync::Arc;

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

pub struct GifWriter<W: Write> {
    encoder: gif::Encoder<W>,
    width: u16,
    height: u16,
    /// The frame waiting to be written, and how long it has been shown so far (ms).
    pending: Option<(Raster, u32)>,
    frames: u32,
    /// NeuQuant speed: 1 is best and slowest, 30 fastest.
    speed: i32,
}

impl<W: Write> GifWriter<W> {
    pub fn new(out: W, width: u32, height: u32) -> Result<GifWriter<W>, GifError> {
        let (w, h) = (u16::try_from(width).map_err(|_| GifError::TooLarge)?, u16::try_from(height).map_err(|_| GifError::TooLarge)?);
        let mut encoder = gif::Encoder::new(out, w, h, &[]).map_err(|e| GifError::Write(e.to_string()))?;
        encoder.set_repeat(gif::Repeat::Infinite).map_err(|e| GifError::Write(e.to_string()))?;
        // Big frames quantise faster, so recording keeps up.
        let speed = if u32::from(w) * u32::from(h) > 1_000_000 { 20 } else { 10 };
        Ok(GifWriter { encoder, width: w, height: h, pending: None, frames: 0, speed })
    }

    /// Adds a frame shown `delay_ms` after the previous one.
    pub fn push(&mut self, frame: Raster, delay_ms: u32) -> Result<(), GifError> {
        if frame.width() != u32::from(self.width) || frame.height() != u32::from(self.height) {
            return Err(GifError::WrongSize);
        }
        match &mut self.pending {
            Some((prev, delay)) if prev.pixels() == frame.pixels() => {
                *delay = delay.saturating_add(delay_ms);
                Ok(())
            }
            _ => {
                // The previous frame lasts until this one comes.
                if let Some((prev, shown)) = self.pending.take() {
                    self.write(prev, shown.saturating_add(delay_ms))?;
                }
                self.pending = Some((frame, 0));
                Ok(())
            }
        }
    }

    fn write(&mut self, frame: Raster, delay_ms: u32) -> Result<(), GifError> {
        let mut rgba = frame.into_pixels();
        let mut f = gif::Frame::from_rgba_speed(self.width, self.height, &mut rgba, self.speed);
        f.delay = u16::try_from((delay_ms / 10).max(2)).unwrap_or(u16::MAX);
        self.encoder.write_frame(&f).map_err(|e| GifError::Write(e.to_string()))?;
        self.frames += 1;
        Ok(())
    }

    /// Writes the last frame (shown for `last_ms`) and the end of the file.
    pub fn finish(mut self, last_ms: u32) -> Result<(W, u32), GifError> {
        if let Some((prev, shown)) = self.pending.take() {
            self.write(prev, shown.saturating_add(last_ms))?;
        }
        let frames = self.frames;
        let out = self.encoder.into_inner().map_err(|e| GifError::Write(e.to_string()))?;
        Ok((out, frames))
    }
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
