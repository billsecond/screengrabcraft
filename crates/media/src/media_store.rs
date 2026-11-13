//! The media bin's files as read: pictures, sound (decoded to 48 kHz stereo), and other
//! recordings (drawn and mixed as they were edited). The host reads each file and hands it here,
//! on a thread of its own if it likes; until then an item shows nothing.
//!
//! Every file is someone else's: sizes are capped, and what can't be read is said, not panicked
//! over.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use screengrabcraft_model::Raster;

use crate::compose::{self, Composer};
use crate::layer_draw::MediaStore;
use crate::layers::MediaKind;
use crate::recording::{Recording, StreamKind};
use crate::timeline::Timeline;

/// The longest sound kept (minutes); longer is cut there.
pub const MAX_SOUND_MINS: u64 = 30;
/// A recording brought in is drawn no larger than this (it's shown over another).
const VIDEO_MAX: (u32, u32) = (1280, 720);

/// A file read for the media bin: what it is, and what's in it.
pub struct Loaded {
    pub kind: MediaKind,
    /// How long (sound, video), µs.
    pub duration: u64,
    pub width: u32,
    pub height: u32,
    pub content: Content,
}

pub enum Content {
    Picture(Raster),
    /// Interleaved stereo at 48 kHz.
    Sound(Vec<i16>),
    Recording(Box<Recording>),
    /// A video file (an MP4): its pictures as the host reads them, and its sound if it has some.
    Clip {
        frames: Box<dyn Frames>,
        sound: Option<Vec<i16>>,
    },
}

/// A video file's pictures, read when they're shown.
pub trait Frames: Send + Sync {
    /// The picture at `t` (µs from its start).
    fn frame_at(&self, t: u64) -> Option<Arc<Raster>>;
}

#[cfg(target_os = "windows")]
impl Frames for crate::clip::Clip {
    fn frame_at(&self, t: u64) -> Option<Arc<Raster>> {
        crate::clip::Clip::frame_at(self, t)
    }
}

impl Loaded {
    /// A recording, as it was edited.
    pub fn recording(rec: Recording) -> Loaded {
        let tl = rec.timeline.clone().map(|t| t.checked(&rec)).unwrap_or_else(|| Timeline::new(&rec));
        let screen = rec
            .header
            .streams
            .iter()
            .find(|s| s.kind == StreamKind::Screen)
            .or_else(|| rec.header.streams.iter().find(|s| s.kind == StreamKind::Camera));
        let (width, height) = screen.map(|s| (s.width, s.height)).unwrap_or((640, 360));
        Loaded { kind: MediaKind::Video, duration: tl.length(), width, height, content: Content::Recording(Box::new(rec)) }
    }
}

/// A file being read for the media bin (on a thread of the host's): what came of it, once it has.
pub type MediaJob = Arc<Mutex<Option<Result<Loaded, String>>>>;

/// How a file of the bin is getting on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaState {
    Loading,
    Ready,
    Failed(String),
}

/// A recording brought in, drawn as it was edited.
struct Video {
    rec: Arc<Recording>,
    timeline: Timeline,
    composer: Mutex<Composer>,
    /// The last picture drawn, and at what time.
    last: Mutex<Option<(u64, Arc<Raster>)>>,
}

enum Entry {
    Loading,
    Failed(String),
    Picture(Arc<Raster>),
    /// Interleaved stereo at 48 kHz.
    Sound(Arc<Vec<i16>>),
    Video(Box<Video>),
    Clip(Box<dyn Frames>, Option<Arc<Vec<i16>>>),
}

/// Interleaved stereo at 48 kHz from `first` (frames), `count` of them, as -1 to 1.
fn sound_of(s: &[i16], first: u64, count: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; count * 2];
    let from = usize::try_from(first.saturating_mul(2)).unwrap_or(usize::MAX);
    if let Some(src) = s.get(from.min(s.len())..) {
        for (o, v) in out.iter_mut().zip(src) {
            *o = f32::from(*v) / 32768.0;
        }
    }
    out
}

/// The media bin, as read so far.
#[derive(Default)]
pub struct MediaLibrary {
    entries: Mutex<HashMap<u32, Entry>>,
}

impl MediaLibrary {
    fn put(&self, id: u32, e: Entry) {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner).insert(id, e);
    }

    /// Being read.
    pub fn loading(&self, id: u32) {
        self.put(id, Entry::Loading);
    }

    pub fn failed(&self, id: u32, why: impl Into<String>) {
        self.put(id, Entry::Failed(why.into()));
    }

    pub fn add_picture(&self, id: u32, pic: Raster) {
        self.put(id, Entry::Picture(Arc::new(pic)));
    }

    /// Sound as interleaved stereo at 48 kHz.
    pub fn add_sound(&self, id: u32, samples: Vec<i16>) {
        self.put(id, Entry::Sound(Arc::new(samples)));
    }

    /// Another recording, as it was edited.
    pub fn add_recording(&self, id: u32, rec: Recording) {
        let rec = Arc::new(rec);
        let timeline = rec.timeline.clone().map(|t| t.checked(&rec)).unwrap_or_else(|| Timeline::new(&rec));
        let (w, h) = rec
            .header
            .streams
            .iter()
            .find(|s| s.kind == StreamKind::Screen)
            .or_else(|| rec.header.streams.iter().find(|s| s.kind == StreamKind::Camera))
            .map(|s| compose::fit(s.width, s.height, VIDEO_MAX.0, VIDEO_MAX.1))
            .unwrap_or((640, 360));
        let composer = Mutex::new(Composer::new(rec.clone(), &timeline, w, h));
        self.put(id, Entry::Video(Box::new(Video { rec, timeline, composer, last: Mutex::new(None) })));
    }

    /// What a file read holds, under its media id.
    pub fn insert(&self, id: u32, content: Content) {
        match content {
            Content::Picture(p) => self.add_picture(id, p),
            Content::Sound(s) => self.add_sound(id, s),
            Content::Recording(r) => self.add_recording(id, *r),
            Content::Clip { frames, sound } => self.put(id, Entry::Clip(frames, sound.map(Arc::new))),
        }
    }

    pub fn state(&self, id: u32) -> Option<MediaState> {
        let e = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        e.get(&id).map(|e| match e {
            Entry::Loading => MediaState::Loading,
            Entry::Failed(w) => MediaState::Failed(w.clone()),
            _ => MediaState::Ready,
        })
    }

    /// Whether a file of the bin has sound to hear (none until it's read).
    pub fn has_sound(&self, id: u32) -> Option<bool> {
        let e = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        match e.get(&id)? {
            Entry::Loading => None,
            Entry::Failed(_) | Entry::Picture(_) => Some(false),
            Entry::Sound(s) => Some(!s.is_empty()),
            Entry::Clip(_, sound) => Some(sound.as_ref().is_some_and(|s| !s.is_empty())),
            Entry::Video(v) => Some(v.rec.header.streams.iter().any(|s| matches!(s.kind, StreamKind::Microphone | StreamKind::SystemSound))),
        }
    }

    /// Whether a file of the bin has been handed here at all (read, being read, or failed).
    pub fn has(&self, id: u32) -> bool {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner).contains_key(&id)
    }
}

impl MediaStore for MediaLibrary {
    fn picture(&self, id: u32, t: u64) -> Option<Arc<Raster>> {
        let e = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        match e.get(&id)? {
            Entry::Picture(p) => Some(p.clone()),
            Entry::Video(v) => {
                let t = t.min(v.timeline.length().saturating_sub(1));
                let mut last = v.last.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some((at, p)) = last.as_ref()
                    && *at == t
                {
                    return Some(p.clone());
                }
                let pic = Arc::new(v.composer.lock().unwrap_or_else(PoisonError::into_inner).frame(&v.timeline, t));
                *last = Some((t, pic.clone()));
                Some(pic)
            }
            Entry::Clip(frames, _) => frames.frame_at(t),
            _ => None,
        }
    }

    fn sound(&self, id: u32, first: u64, count: usize) -> Vec<f32> {
        let e = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        match e.get(&id) {
            Some(Entry::Sound(s)) => sound_of(s, first, count),
            Some(Entry::Clip(_, Some(s))) => sound_of(s, first, count),
            Some(Entry::Video(v)) => compose::mix(&v.rec, &v.timeline, first, count),
            _ => vec![0.0; count * 2],
        }
    }
}

/// What a file is, by its name: a picture, sound, a recording, or a video file.
pub fn kind_of(name: &str) -> Option<MediaKind> {
    let ext = std::path::Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" => Some(MediaKind::Picture),
        "mp3" | "wav" | "m4a" | "aac" | "flac" | "ogg" | "oga" => Some(MediaKind::Sound),
        "sgvideo" | "mp4" | "m4v" | "mov" => Some(MediaKind::Video),
        _ => None,
    }
}

/// The file kinds the media bin takes, for an open dialog.
pub const EXTENSIONS: &[&str] =
    &["png", "jpg", "jpeg", "gif", "bmp", "webp", "mp3", "wav", "m4a", "aac", "flac", "ogg", "oga", "sgvideo", "mp4", "m4v", "mov"];

/// Sound read from a file's bytes (`ext` its kind): interleaved stereo at 48 kHz, at most
/// [`MAX_SOUND_MINS`] long, and how long it is (µs).
#[cfg(not(target_arch = "wasm32"))]
pub fn decode_sound(bytes: Vec<u8>, ext: &str) -> Result<(Vec<i16>, u64), String> {
    decode_sound_from(Box::new(std::io::Cursor::new(bytes)), ext)
}

/// [`decode_sound`] from a file as it's read (a video file can be large).
#[cfg(not(target_arch = "wasm32"))]
pub fn decode_sound_file(file: std::fs::File, ext: &str) -> Result<(Vec<i16>, u64), String> {
    decode_sound_from(Box::new(file), ext)
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_sound_from(source: Box<dyn symphonia::core::io::MediaSource>, ext: &str) -> Result<(Vec<i16>, u64), String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let stream = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);
    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("not a sound file this program reads ({e})"))?;
    let mut format = probed.format;
    let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or("it has no sound")?;
    let track_id = track.id;
    let rate = track.codec_params.sample_rate.unwrap_or(48_000).clamp(4_000, 384_000);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("its sound can't be read ({e})"))?;
    // Read as interleaved f32 at its own rate, both channels (mono doubled, more mixed down).
    let most = MAX_SOUND_MINS * 60 * u64::from(rate);
    let mut stereo: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(e) => {
                if stereo.is_empty() {
                    return Err(format!("its sound can't be read ({e})"));
                }
                break;
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // A damaged packet is skipped.
            Err(Error::DecodeError(_)) => continue,
            Err(e) => {
                if stereo.is_empty() {
                    return Err(format!("its sound can't be read ({e})"));
                }
                break;
            }
        };
        let spec = *decoded.spec();
        let ch = spec.channels.count().max(1);
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);
        for frame in buf.samples().chunks(ch) {
            let (l, r) = match frame {
                [m] => (*m, *m),
                [l, r, ..] => (*l, *r),
                [] => (0.0, 0.0),
            };
            stereo.push(l);
            stereo.push(r);
        }
        if stereo.len() as u64 / 2 >= most {
            break;
        }
    }
    if stereo.is_empty() {
        return Err("it has no sound".to_string());
    }
    let samples = resample(&stereo, rate);
    let duration = compose::to_us(samples.len() as u64 / 2);
    Ok((samples, duration))
}

/// Interleaved stereo at `rate` to 16-bit at 48 kHz (straight lines between the samples).
#[cfg(not(target_arch = "wasm32"))]
fn resample(stereo: &[f32], rate: u32) -> Vec<i16> {
    let to16 = |v: f32| (v.clamp(-1.0, 1.0) * 32767.0) as i16;
    let frames = stereo.len() / 2;
    if rate == 48_000 {
        return stereo.iter().map(|v| to16(*v)).collect();
    }
    let out_frames = (frames as u64 * 48_000 / u64::from(rate.max(1))) as usize;
    let step = f64::from(rate) / 48_000.0;
    let mut out = Vec::with_capacity(out_frames * 2);
    for k in 0..out_frames {
        let pos = k as f64 * step;
        let i = pos.floor() as usize;
        let f = (pos - i as f64) as f32;
        for c in 0..2 {
            let a = stereo.get(i * 2 + c).copied().unwrap_or(0.0);
            let b = stereo.get((i + 1) * 2 + c).copied().unwrap_or(a);
            out.push(to16(a + (b - a) * f));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV file of `secs` of a tone at `rate`, mono.
    fn wav(rate: u32, secs: f32) -> Vec<u8> {
        let n = (rate as f32 * secs) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + n * 2).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(n * 2).to_le_bytes());
        for i in 0..n {
            let v = ((i as f32 * 0.05).sin() * 12_000.0) as i16;
            b.extend_from_slice(&v.to_le_bytes());
        }
        b
    }

    #[test]
    fn a_wav_is_read_at_48_khz_in_stereo() {
        let (s, d) = decode_sound(wav(24_000, 1.0), "wav").unwrap();
        assert_eq!(s.len(), 96_000, "a second, stereo, at 48 kHz");
        assert!((d as i64 - 1_000_000).abs() < 1_000);
        assert!(s.iter().any(|v| v.unsigned_abs() > 5_000));
        assert_eq!(s[100], s[101], "mono heard on both sides");
    }

    #[test]
    fn junk_is_said_not_panicked_over() {
        for junk in [Vec::new(), b"RIFF\x00\x00\x00\x00WAVE".to_vec(), vec![0xFF; 4096], wav(48_000, 0.5)[..60].to_vec()] {
            let _ = decode_sound(junk, "wav");
        }
        assert!(decode_sound(vec![1, 2, 3], "mp3").is_err());
        assert_eq!(kind_of("Song.MP3"), Some(MediaKind::Sound));
        assert_eq!(kind_of("Logo.png"), Some(MediaKind::Picture));
        assert_eq!(kind_of("Capture.sgvideo"), Some(MediaKind::Video));
        assert_eq!(kind_of("notes.txt"), None);
    }

    #[test]
    fn the_library_gives_what_it_was_given() {
        let lib = MediaLibrary::default();
        assert!(lib.picture(1, 0).is_none() && !lib.has(1));
        lib.loading(1);
        assert_eq!(lib.state(1), Some(MediaState::Loading));
        lib.add_picture(1, Raster::filled(4, 4, screengrabcraft_model::Color::WHITE).unwrap());
        assert_eq!(lib.state(1), Some(MediaState::Ready));
        assert_eq!(lib.picture(1, 123).unwrap().width(), 4);
        lib.add_sound(2, vec![16_384; 20]);
        let s = lib.sound(2, 5, 10);
        assert_eq!(s.len(), 20);
        assert!((s[0] - 0.5).abs() < 1e-3 && s[10] == 0.0, "after its end, silence");
        lib.failed(3, "can't be read");
        assert_eq!(lib.state(3), Some(MediaState::Failed("can't be read".into())));
        assert_eq!(lib.sound(3, 0, 4), vec![0.0; 8]);
    }
}
