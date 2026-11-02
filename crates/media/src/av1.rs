//! AV1 video with rav1e: royalty-free, written in Rust, and small files at a good quality.
//!
//! rav1e is careful rather than fast, so a long video is made in stretches that each start with
//! a key frame and are encoded side by side (see [`crate::export`]). Every stretch has the same
//! settings, so they join into one stream.

use rav1e::prelude::*;

use crate::MediaError;
use crate::yuv::I420;

/// How a video is encoded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Av1Settings {
    /// Even, at least 16 × 16.
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// rav1e's quantizer, 0 (best) to 255 (smallest): about 100 to 140 suits screen recordings.
    pub quantizer: u8,
    /// rav1e's speed preset, 0 (slowest) to 10 (fastest).
    pub speed: u8,
    /// Threads for this one encoder (0: rav1e decides).
    pub threads: usize,
}

/// One coded picture: its bytes (without temporal delimiters, as MP4 wants), and whether it's
/// a key frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coded {
    pub data: Vec<u8>,
    pub key: bool,
}

pub struct Av1Encoder {
    ctx: Context<u8>,
    width: u32,
    height: u32,
}

fn err(e: impl std::fmt::Display) -> MediaError {
    MediaError::Encode(e.to_string())
}

impl Av1Encoder {
    pub fn new(s: Av1Settings) -> Result<Av1Encoder, MediaError> {
        if s.width < 16 || s.height < 16 || s.width % 2 == 1 || s.height % 2 == 1 {
            return Err(MediaError::Encode(format!("a video can't be {} × {}", s.width, s.height)));
        }
        let mut enc = EncoderConfig::with_speed_preset(s.speed.min(10));
        enc.width = s.width as usize;
        enc.height = s.height as usize;
        enc.time_base = Rational::new(1, u64::from(s.fps.clamp(1, 120)));
        enc.bit_depth = 8;
        enc.chroma_sampling = ChromaSampling::Cs420;
        enc.pixel_range = PixelRange::Limited;
        enc.color_description = Some(ColorDescription {
            color_primaries: ColorPrimaries::BT709,
            transfer_characteristics: TransferCharacteristics::BT709,
            matrix_coefficients: MatrixCoefficients::BT709,
        });
        enc.quantizer = usize::from(s.quantizer);
        enc.min_quantizer = 0;
        enc.bitrate = 0;
        enc.low_latency = true;
        // Key frames only where a stretch starts (each stretch has its own encoder).
        enc.min_key_frame_interval = 0;
        enc.max_key_frame_interval = 100_000;
        enc.tiles = 16;
        let cfg = Config::new().with_encoder_config(enc).with_threads(s.threads);
        let ctx = cfg.new_context().map_err(err)?;
        Ok(Av1Encoder { ctx, width: s.width, height: s.height })
    }

    /// The `av1C` record for the MP4 file: the four bytes rav1e gives, then the sequence header
    /// from a key frame (players want it there too).
    pub fn config_record(&self, key_frame: &Coded) -> Vec<u8> {
        let mut record = self.ctx.container_sequence_header();
        record.extend(obus(&key_frame.data).filter(|o| o.first().is_some_and(|h| (h >> 3) & 0x0F == 1)).flat_map(|o| o.to_vec()));
        record
    }

    /// Takes the next picture; whatever is coded by now is returned.
    pub fn send(&mut self, frame: &I420) -> Result<Vec<Coded>, MediaError> {
        if frame.width != self.width || frame.height != self.height {
            return Err(MediaError::Encode("a picture of the wrong size".to_string()));
        }
        let mut f = self.ctx.new_frame();
        let w = self.width as usize;
        f.planes[0].copy_from_raw_u8(&frame.y, w, 1);
        f.planes[1].copy_from_raw_u8(&frame.u, w / 2, 1);
        f.planes[2].copy_from_raw_u8(&frame.v, w / 2, 1);
        let mut out = Vec::new();
        loop {
            match self.ctx.send_frame(f.clone()) {
                Ok(()) => break,
                Err(EncoderStatus::EnoughData) => self.drain(&mut out)?,
                Err(e) => return Err(err(e)),
            }
        }
        self.drain(&mut out)?;
        Ok(out)
    }

    /// No more pictures: everything still inside is coded and returned.
    pub fn finish(&mut self) -> Result<Vec<Coded>, MediaError> {
        self.ctx.flush();
        let mut out = Vec::new();
        loop {
            match self.ctx.receive_packet() {
                Ok(p) => out.push(Coded { data: strip_temporal_delimiters(&p.data), key: p.frame_type == FrameType::KEY }),
                Err(EncoderStatus::Encoded) => {}
                Err(EncoderStatus::LimitReached) => return Ok(out),
                Err(e) => return Err(err(e)),
            }
        }
    }

    fn drain(&mut self, out: &mut Vec<Coded>) -> Result<(), MediaError> {
        loop {
            match self.ctx.receive_packet() {
                Ok(p) => out.push(Coded { data: strip_temporal_delimiters(&p.data), key: p.frame_type == FrameType::KEY }),
                Err(EncoderStatus::Encoded) => {}
                Err(EncoderStatus::NeedMoreData) => return Ok(()),
                Err(e) => return Err(err(e)),
            }
        }
    }
}

/// An AV1 temporal unit without its temporal delimiter OBUs (an MP4 sample mustn't have them).
/// Anything that doesn't parse is kept as it is.
pub fn strip_temporal_delimiters(tu: &[u8]) -> Vec<u8> {
    obus(tu).filter(|o| o.first().is_none_or(|h| (h >> 3) & 0x0F != 2)).flat_map(|o| o.to_vec()).collect()
}

/// The OBUs of a temporal unit, in order; a part that doesn't parse comes out whole, last.
fn obus(tu: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = tu;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let len = obu_len(rest).unwrap_or(rest.len());
        let (obu, after) = rest.split_at(len.min(rest.len()));
        rest = after;
        Some(obu)
    })
}

/// The length of the OBU at the start (header, size field and payload), if it has a size field.
fn obu_len(b: &[u8]) -> Option<usize> {
    let header = *b.first()?;
    let ext = (header >> 2) & 1 == 1;
    let has_size = (header >> 1) & 1 == 1;
    if !has_size {
        return None;
    }
    let mut at = if ext { 2 } else { 1 };
    let mut size = 0u64;
    for i in 0..8 {
        let byte = *b.get(at)?;
        at += 1;
        size |= u64::from(byte & 0x7F) << (7 * i);
        if byte & 0x80 == 0 {
            let total = at.checked_add(usize::try_from(size).ok()?)?;
            return (total <= b.len()).then_some(total);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_delimiters_come_out_and_the_rest_stays() {
        // A temporal delimiter (type 2, size 0), then a frame OBU (type 6) with 3 bytes.
        let tu = [0x12, 0x00, 0x32, 0x03, 9, 8, 7];
        assert_eq!(strip_temporal_delimiters(&tu), vec![0x32, 0x03, 9, 8, 7]);
        // Junk is kept rather than lost.
        assert_eq!(strip_temporal_delimiters(&[0x32, 0x09, 1]), vec![0x32, 0x09, 1]);
        assert_eq!(strip_temporal_delimiters(&[]), Vec::<u8>::new());
    }

    #[test]
    fn a_few_pictures_come_out_starting_with_a_key_frame() {
        let s = Av1Settings { width: 64, height: 48, fps: 30, quantizer: 120, speed: 10, threads: 1 };
        let mut e = Av1Encoder::new(s).unwrap();
        let mut coded = Vec::new();
        for i in 0..5u8 {
            let mut f = I420::black(64, 48);
            f.y.iter_mut().take(64 * usize::from(i + 1)).for_each(|p| *p = 200);
            coded.extend(e.send(&f).unwrap());
        }
        coded.extend(e.finish().unwrap());
        assert_eq!(coded.len(), 5);
        assert!(coded[0].key);
        assert!(coded.iter().skip(1).all(|c| !c.key));
        assert!(coded.iter().all(|c| !c.data.is_empty() && c.data[0] >> 3 & 0x0F != 2));
        // The record: marker and version, then a sequence header OBU (type 1).
        let record = e.config_record(&coded[0]);
        assert_eq!(record[0], 0x81);
        assert!(record.len() > 6 && (record[4] >> 3) & 0x0F == 1);
    }

    #[test]
    fn sizes_a_video_cant_have_are_refused() {
        let s = |w, h| Av1Settings { width: w, height: h, fps: 30, quantizer: 120, speed: 10, threads: 1 };
        assert!(Av1Encoder::new(s(63, 48)).is_err());
        assert!(Av1Encoder::new(s(8, 8)).is_err());
    }
}
