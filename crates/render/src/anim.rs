//! Animated GIFs from recorded frames.
//!
//! Frames come in as they are captured, each with the time since the one before. A frame that
//! is the same as the one before only makes that one last longer, so a still screen costs
//! nothing. Each frame gets its own palette of 256 colours (NeuQuant, which keeps text and
//! interface colours well).

use std::io::Write;

use screengrabcraft_model::Raster;

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

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Color;

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
