//! Opus sound with rusty-opus: royalty-free, written in Rust, clear speech and music at a
//! small size.

use crate::MediaError;

/// Samples a second: everything is recorded and made at 48 kHz.
pub const RATE: u32 = 48_000;
/// One Opus packet: 20 ms.
pub const FRAME: usize = 960;
/// The encoder's warm-up at 48 kHz (its lookahead), which a player skips. Measured, and held
/// by a test: the decoded sound lines up with what went in this many samples later.
pub const PRE_SKIP: u16 = 120;

pub struct OpusEncoder {
    enc: rusty_opus::OpusEncoder,
    channels: usize,
    pending: Vec<f32>,
    buf: Vec<u8>,
}

impl OpusEncoder {
    /// `channels` is 1 or 2; `bitrate` in bits a second.
    pub fn new(channels: u8, bitrate: u32) -> Result<OpusEncoder, MediaError> {
        let ch = usize::from(channels);
        let mut enc =
            rusty_opus::OpusEncoder::new(RATE as i32, ch, rusty_opus::Application::Audio).map_err(|e| MediaError::Encode(format!("sound: {e}")))?;
        enc.bitrate_bps = bitrate.clamp(16_000, 256_000) as i32;
        Ok(OpusEncoder { enc, channels: ch, pending: Vec::new(), buf: vec![0; 4000] })
    }

    /// Takes interleaved samples (-1 to 1); returns the packets made (each 20 ms).
    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<Vec<u8>>, MediaError> {
        self.pending.extend(samples.iter().map(|s| if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 }));
        let step = FRAME * self.channels;
        let mut out = Vec::new();
        let mut at = 0;
        while self.pending.len() - at >= step {
            let frame = self.pending.get(at..at + step).unwrap_or_default();
            let n = self.enc.encode(frame, FRAME, &mut self.buf).map_err(|e| MediaError::Encode(format!("sound: {e}")))?;
            out.push(self.buf.get(..n).unwrap_or_default().to_vec());
            at += step;
        }
        self.pending.drain(..at);
        Ok(out)
    }

    /// The last samples, made up to a whole packet with silence, plus enough silence after
    /// them that the warm-up doesn't cut the end off.
    pub fn finish(&mut self) -> Result<Vec<Vec<u8>>, MediaError> {
        let step = FRAME * self.channels;
        let tail = usize::from(PRE_SKIP) * self.channels;
        let mut pad = step - (self.pending.len() + tail) % step;
        if pad == step {
            pad = 0;
        }
        let silence = vec![0.0f32; tail + pad];
        self.push(&silence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_of_sound_makes_fifty_packets_and_a_tail() {
        let mut e = OpusEncoder::new(2, 96_000).unwrap();
        let tone: Vec<f32> = (0..RATE as usize * 2).map(|i| ((i / 2) as f32 * 0.0575).sin() * 0.3).collect();
        let mut packets = e.push(&tone).unwrap();
        assert_eq!(packets.len(), 50);
        packets.extend(e.finish().unwrap());
        assert_eq!(packets.len(), 51);
        assert!(packets.iter().all(|p| !p.is_empty() && p.len() < 1500));
    }

    #[test]
    fn what_comes_back_out_sounds_like_what_went_in() {
        let mut e = OpusEncoder::new(1, 64_000).unwrap();
        // Loud and quiet in turn every 100 ms, so only the true delay lines it up.
        let loudness = |i: usize| if (i / 4800).is_multiple_of(2) { 0.5 } else { 0.1 };
        let tone: Vec<f32> = (0..RATE as usize).map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / RATE as f32).sin() * loudness(i)).collect();
        let mut packets = e.push(&tone).unwrap();
        packets.extend(e.finish().unwrap());
        let mut d = rusty_opus::OpusDecoder::new(RATE as i32, 1).unwrap();
        let mut decoded = Vec::new();
        let mut frame = vec![0f32; FRAME];
        for p in &packets {
            let n = d.decode(p, FRAME, &mut frame).unwrap();
            decoded.extend_from_slice(&frame[..n]);
        }
        let a = &tone[4800..43200];
        let off = |lag: usize| a.iter().zip(&decoded[4800 + lag..43200 + lag]).map(|(x, y)| (x - y).powi(2)).sum::<f32>() / a.len() as f32;
        let best = (0..1000).min_by(|x, y| off(*x).total_cmp(&off(*y))).unwrap();
        assert_eq!(best, usize::from(PRE_SKIP), "the warm-up changed");
        let power: f32 = a.iter().map(|x| x * x).sum::<f32>() / a.len() as f32;
        assert!(off(best) < power * 0.01, "the decoded tone is off: {} against {power}", off(best));
    }
}
