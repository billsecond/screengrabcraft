//! The recording file (`.sgvideo`): everything a video recording captured, each source on a
//! track of its own (the screen, the camera, the computer's sound, the microphone), and the
//! edits made since.
//!
//! The file is only ever added to, chunk by chunk, so a recording that stopped halfway (the
//! program or the computer went down) still opens with everything written up to then. Edits
//! are added as chunks too; the last one counts.
//!
//! ```text
//! file  := "SGVIDEO\x01" chunk*
//! chunk := tag[4] length:u32 crc32:u32 payload[length]       (little-endian)
//! HEAD  JSON Header: the tracks
//! FRAM  stream:u8 kind:u8 pts:u64 (µs) then
//!         kind 0 (key):   width:u32 height:u32 lz4(RGB of the whole picture)
//!         kind 1 (delta): x:u32 y:u32 w:u32 h:u32 lz4(RGB of the part that changed)
//!         kind 2 (JPEG):  the picture as a JPEG (the camera)
//! AUDI  stream:u8 first:u64 (sample number at 48 kHz) i16 samples, interleaved
//! EDIT  JSON Timeline
//! DONE  JSON Finished
//! ```
//!
//! Everything read from a file is checked: sizes are capped, a chunk that's cut short or whose
//! checksum is wrong ends the reading there, and a frame that doesn't fit is skipped.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::sync::Mutex;

use screengrabcraft_model::Raster;
use serde::{Deserialize, Serialize};

use crate::timeline::Timeline;

pub const EXT: &str = "sgvideo";
pub const MAGIC: &[u8; 8] = b"SGVIDEO\x01";
/// A screen picture is written whole at least this often, so jumping anywhere is quick.
pub const KEY_EVERY_US: u64 = 2_000_000;
/// The largest chunk read.
const MAX_CHUNK: u32 = 256 * 1024 * 1024;
/// The largest picture on a track.
const MAX_SIDE: u32 = 16_384;
const MAX_TRACKS: usize = 8;
/// The longest recording read (a damaged file can claim anything).
const MAX_DURATION_US: u64 = 24 * 3600 * 1_000_000;

#[derive(Debug, thiserror::Error)]
pub enum RecordingError {
    #[error("this is not a ScreenGrabCraft video recording")]
    NotARecording,
    #[error("the recording is damaged: {0}")]
    Damaged(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamKind {
    Screen,
    Camera,
    /// What the computer played (its speakers' sound).
    SystemSound,
    Microphone,
}

impl StreamKind {
    pub fn is_video(self) -> bool {
        matches!(self, StreamKind::Screen | StreamKind::Camera)
    }

    pub fn label(self) -> &'static str {
        match self {
            StreamKind::Screen => "Screen",
            StreamKind::Camera => "Camera",
            StreamKind::SystemSound => "Computer sound",
            StreamKind::Microphone => "Microphone",
        }
    }
}

/// One track of a recording.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub id: u8,
    pub kind: StreamKind,
    /// The device's name (a camera's or a microphone's), for the owner.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    /// Sound: 1 or 2.
    #[serde(default)]
    pub channels: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub version: u32,
    /// When it was recorded, in local time (`2026-11-02T10:15:30`).
    #[serde(default)]
    pub created: String,
    /// The frame rate the screen was recorded at.
    #[serde(default = "default_fps")]
    pub fps: u32,
    pub streams: Vec<StreamInfo>,
}

fn default_fps() -> u32 {
    30
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finished {
    pub duration_us: u64,
}

// ---- writing ----

struct VideoState {
    /// The last picture, RGB.
    last: Vec<u8>,
    width: u32,
    height: u32,
    last_key_us: u64,
}

/// Writes a recording as it happens. Every call writes its chunk at once.
pub struct RecordingWriter<W: Write> {
    out: W,
    video: BTreeMap<u8, VideoState>,
    written: u64,
}

fn chunk(out: &mut impl Write, tag: &[u8; 4], payload: &[u8]) -> io::Result<u64> {
    let len = u32::try_from(payload.len()).ok().filter(|l| *l <= MAX_CHUNK).ok_or_else(|| io::Error::other("a chunk is too large"))?;
    out.write_all(tag)?;
    out.write_all(&len.to_le_bytes())?;
    out.write_all(&crc32fast::hash(payload).to_le_bytes())?;
    out.write_all(payload)?;
    Ok(12 + u64::from(len))
}

fn rgb(raster: &Raster) -> Vec<u8> {
    raster.pixels().as_chunks::<4>().0.iter().flat_map(|&[r, g, b, _]| [r, g, b]).collect()
}

/// The smallest rectangle holding every pixel that differs, if any does.
fn changed(before: &[u8], after: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let row = w as usize * 3;
    let mut rows = before.chunks_exact(row).zip(after.chunks_exact(row)).enumerate().filter(|(_, (a, b))| a != b);
    let (top, _) = rows.next()?;
    let mut bottom = top;
    let (mut left, mut right) = (w as usize, 0usize);
    for (y, (a, b)) in std::iter::once((top, (before.chunks_exact(row).nth(top)?, after.chunks_exact(row).nth(top)?))).chain(rows) {
        bottom = y;
        let first = a.as_chunks::<3>().0.iter().zip(b.as_chunks::<3>().0).position(|(p, q)| p != q).unwrap_or(0);
        let last = a.as_chunks::<3>().0.iter().zip(b.as_chunks::<3>().0).rposition(|(p, q)| p != q).unwrap_or(0);
        left = left.min(first);
        right = right.max(last);
    }
    let _ = h;
    Some((left as u32, top as u32, (right - left + 1) as u32, (bottom - top + 1) as u32))
}

fn crop_rgb(px: &[u8], w: u32, x: u32, y: u32, cw: u32, ch: u32) -> Vec<u8> {
    let row = w as usize * 3;
    px.chunks_exact(row)
        .skip(y as usize)
        .take(ch as usize)
        .flat_map(|r| r.get(x as usize * 3..(x + cw) as usize * 3).unwrap_or_default().iter().copied())
        .collect()
}

impl<W: Write> RecordingWriter<W> {
    pub fn new(mut out: W, header: &Header) -> Result<RecordingWriter<W>, RecordingError> {
        out.write_all(MAGIC)?;
        let json = serde_json::to_vec(header).map_err(|e| RecordingError::Damaged(e.to_string()))?;
        let n = chunk(&mut out, b"HEAD", &json)?;
        Ok(RecordingWriter { out, video: BTreeMap::new(), written: MAGIC.len() as u64 + n })
    }

    /// Bytes written so far.
    pub fn written(&self) -> u64 {
        self.written
    }

    /// A picture of a screen track at `pts` µs. Only what changed is written; nothing at all if
    /// nothing did.
    pub fn video_frame(&mut self, stream: u8, pts: u64, raster: &Raster) -> Result<(), RecordingError> {
        let (w, h) = (raster.width(), raster.height());
        let px = rgb(raster);
        let key = match self.video.get(&stream) {
            Some(s) => s.width != w || s.height != h || pts.saturating_sub(s.last_key_us) >= KEY_EVERY_US,
            None => true,
        };
        let mut payload = vec![stream, 0];
        payload.extend_from_slice(&pts.to_le_bytes());
        if key {
            payload.extend_from_slice(&w.to_le_bytes());
            payload.extend_from_slice(&h.to_le_bytes());
            payload.extend(lz4_flex::compress_prepend_size(&px));
            self.video.insert(stream, VideoState { last: px, width: w, height: h, last_key_us: pts });
        } else {
            let Some(s) = self.video.get_mut(&stream) else { return Ok(()) };
            let Some((x, y, cw, ch)) = changed(&s.last, &px, w, h) else { return Ok(()) };
            if let Some(k) = payload.get_mut(1) {
                *k = 1;
            }
            for v in [x, y, cw, ch] {
                payload.extend_from_slice(&v.to_le_bytes());
            }
            payload.extend(lz4_flex::compress_prepend_size(&crop_rgb(&px, w, x, y, cw, ch)));
            s.last = px;
        }
        self.written += chunk(&mut self.out, b"FRAM", &payload)?;
        Ok(())
    }

    /// A camera picture as a JPEG.
    pub fn jpeg_frame(&mut self, stream: u8, pts: u64, jpeg: &[u8]) -> Result<(), RecordingError> {
        let mut payload = vec![stream, 2];
        payload.extend_from_slice(&pts.to_le_bytes());
        payload.extend_from_slice(jpeg);
        self.written += chunk(&mut self.out, b"FRAM", &payload)?;
        Ok(())
    }

    /// Sound: interleaved samples starting at sample number `first` (48 kHz, from the start).
    pub fn audio(&mut self, stream: u8, first: u64, samples: &[i16]) -> Result<(), RecordingError> {
        let mut payload = Vec::with_capacity(9 + samples.len() * 2);
        payload.push(stream);
        payload.extend_from_slice(&first.to_le_bytes());
        payload.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
        self.written += chunk(&mut self.out, b"AUDI", &payload)?;
        Ok(())
    }

    /// The edits so far (the last ones written count).
    pub fn edit(&mut self, timeline: &Timeline) -> Result<(), RecordingError> {
        let json = serde_json::to_vec(timeline).map_err(|e| RecordingError::Damaged(e.to_string()))?;
        self.written += chunk(&mut self.out, b"EDIT", &json)?;
        Ok(())
    }

    /// Marks the recording as finished and hands the output back.
    pub fn finish(mut self, duration_us: u64) -> Result<W, RecordingError> {
        let json = serde_json::to_vec(&Finished { duration_us }).map_err(|e| RecordingError::Damaged(e.to_string()))?;
        chunk(&mut self.out, b"DONE", &json)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

/// Adds the edits to a finished recording file (opened for appending).
pub fn append_edit(out: &mut impl Write, timeline: &Timeline) -> Result<(), RecordingError> {
    let json = serde_json::to_vec(timeline).map_err(|e| RecordingError::Damaged(e.to_string()))?;
    chunk(out, b"EDIT", &json)?;
    out.flush()?;
    Ok(())
}

// ---- reading ----

/// Where a recording is read from: its bytes in memory, or a file read piece by piece.
pub trait Source: Send + Sync {
    fn len(&self) -> u64;
    fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>>;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Source for Vec<u8> {
    fn len(&self) -> u64 {
        self.as_slice().len() as u64
    }
    fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let start = usize::try_from(offset).map_err(io::Error::other)?;
        self.get(start..start.saturating_add(len)).map(<[u8]>::to_vec).ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))
    }
}

/// A recording file, read where it's needed.
pub struct FileSource {
    file: Mutex<std::fs::File>,
    len: u64,
}

impl FileSource {
    pub fn open(path: &std::path::Path) -> io::Result<FileSource> {
        let file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        Ok(FileSource { file: Mutex::new(file), len })
    }
}

impl Source for FileSource {
    fn len(&self) -> u64 {
        self.len
    }
    fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = self.file.lock().map_err(|_| io::Error::other("the file is in use"))?;
        f.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0; len];
        f.read_exact(&mut buf)?;
        Ok(buf)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Key,
    Delta,
    Jpeg,
}

/// Where a picture is in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameEntry {
    pub pts: u64,
    pub kind: FrameKind,
    offset: u64,
    len: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AudioEntry {
    first: u64,
    /// Samples per channel.
    count: u64,
    offset: u64,
    len: u32,
}

/// A recording, opened: its tracks and where everything is.
pub struct Recording {
    source: Box<dyn Source>,
    pub header: Header,
    frames: BTreeMap<u8, Vec<FrameEntry>>,
    audio: BTreeMap<u8, Vec<AudioEntry>>,
    /// The last edits saved, if any.
    pub timeline: Option<Timeline>,
    /// Whether it was finished properly (else it stopped halfway and is what was saved).
    pub finished: bool,
    /// How long it is, in µs.
    pub duration_us: u64,
}

/// Where a stretch of sound ends, in µs.
fn sound_end_us(a: &AudioEntry) -> u64 {
    (a.first.saturating_add(a.count).saturating_mul(1_000_000) / 48_000).min(MAX_DURATION_US)
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

impl Recording {
    pub fn open(source: Box<dyn Source>) -> Result<Recording, RecordingError> {
        let len = source.len();
        if len < 8 || source.read_at(0, 8)?.as_slice() != MAGIC {
            return Err(RecordingError::NotARecording);
        }
        let mut at = 8u64;
        let mut header: Option<Header> = None;
        let mut frames: BTreeMap<u8, Vec<FrameEntry>> = BTreeMap::new();
        let mut audio: BTreeMap<u8, Vec<AudioEntry>> = BTreeMap::new();
        let mut timeline = None;
        let mut finished = None;
        while at + 12 <= len {
            let head = source.read_at(at, 12)?;
            let (Some(size), Some(crc)) = (u32_at(&head, 4), u32_at(&head, 8)) else { break };
            let tag: [u8; 4] = head.get(..4).and_then(|t| t.try_into().ok()).unwrap_or([0; 4]);
            if size > MAX_CHUNK || at + 12 + u64::from(size) > len {
                log::warn!("a recording ends with a chunk cut short");
                break;
            }
            let body_at = at + 12;
            // Pictures and sound are checked when read; the rest now.
            let small = !matches!(&tag, b"FRAM" | b"AUDI");
            let peek = if small { source.read_at(body_at, size as usize)? } else { source.read_at(body_at, (size as usize).min(17))? };
            if small && crc32fast::hash(&peek) != crc {
                log::warn!("a recording has a damaged chunk; reading stops there");
                break;
            }
            match &tag {
                b"HEAD" => {
                    let h: Header = serde_json::from_slice(&peek).map_err(|e| RecordingError::Damaged(e.to_string()))?;
                    header = Some(h);
                }
                b"FRAM" => {
                    let (Some(&stream), Some(&kind), Some(pts)) = (peek.first(), peek.get(1), u64_at(&peek, 2)) else { break };
                    let kind = match kind {
                        0 => FrameKind::Key,
                        1 => FrameKind::Delta,
                        _ => FrameKind::Jpeg,
                    };
                    let list = frames.entry(stream).or_default();
                    // Out of order means damage; such a picture is left out.
                    if list.last().is_none_or(|f| f.pts <= pts) {
                        list.push(FrameEntry { pts, kind, offset: body_at, len: size });
                    }
                }
                b"AUDI" => {
                    let (Some(&stream), Some(first)) = (peek.first(), u64_at(&peek, 1)) else { break };
                    let channels = header
                        .as_ref()
                        .and_then(|h| h.streams.iter().find(|s| s.id == stream))
                        .map(|s| u64::from(s.channels.clamp(1, 2)))
                        .unwrap_or(1);
                    let count = u64::from(size.saturating_sub(9)) / 2 / channels;
                    audio.entry(stream).or_default().push(AudioEntry { first, count, offset: body_at, len: size });
                }
                b"EDIT" => match serde_json::from_slice::<Timeline>(&peek) {
                    Ok(t) => timeline = Some(t),
                    Err(e) => log::warn!("edits in a recording could not be read: {e}"),
                },
                b"DONE" => finished = serde_json::from_slice::<Finished>(&peek).ok(),
                _ => {}
            }
            at = body_at + u64::from(size);
        }
        let mut header = header.ok_or(RecordingError::NotARecording)?;
        header.streams.truncate(MAX_TRACKS);
        header.streams.retain(|s| !s.kind.is_video() || (s.width <= MAX_SIDE && s.height <= MAX_SIDE));
        for list in audio.values_mut() {
            list.sort_by_key(|a| a.first);
        }
        let last_frame = frames.values().filter_map(|f| f.last()).map(|f| f.pts).max().unwrap_or(0);
        let last_sound = audio.values().filter_map(|a| a.last()).map(sound_end_us).max().unwrap_or(0);
        let duration_us = match &finished {
            Some(f) => f.duration_us,
            None => last_frame.max(last_sound),
        }
        .min(MAX_DURATION_US);
        let mut rec = Recording { source, header, frames, audio, timeline, finished: finished.is_some(), duration_us };
        if let Some(t) = rec.timeline.take() {
            rec.timeline = Some(t.checked(&rec));
        }
        Ok(rec)
    }

    pub fn stream(&self, id: u8) -> Option<&StreamInfo> {
        self.header.streams.iter().find(|s| s.id == id)
    }

    /// The pictures of a track, in order.
    pub fn frames(&self, stream: u8) -> &[FrameEntry] {
        self.frames.get(&stream).map(Vec::as_slice).unwrap_or_default()
    }

    /// A track's length in µs (to its last picture or sample; the whole recording at least for
    /// the screen).
    pub fn stream_duration(&self, stream: u8) -> u64 {
        match self.stream(stream) {
            Some(s) if s.kind.is_video() => self.duration_us,
            Some(_) => self.audio.get(&stream).and_then(|a| a.last()).map(sound_end_us).unwrap_or(0),
            None => 0,
        }
    }

    fn payload(&self, offset: u64, len: u32) -> Result<Vec<u8>, RecordingError> {
        let body = self.source.read_at(offset, len as usize)?;
        Ok(body)
    }

    /// A key or delta picture's data, checked: (x, y, w, h, RGB).
    fn screen_part(&self, f: &FrameEntry) -> Result<(u32, u32, u32, u32, Vec<u8>), RecordingError> {
        let b = self.payload(f.offset, f.len)?;
        let bad = || RecordingError::Damaged("a picture doesn't fit".to_string());
        let (x, y, w, h, data_at) = match f.kind {
            FrameKind::Key => (0, 0, u32_at(&b, 10).ok_or_else(bad)?, u32_at(&b, 14).ok_or_else(bad)?, 18),
            FrameKind::Delta => (
                u32_at(&b, 10).ok_or_else(bad)?,
                u32_at(&b, 14).ok_or_else(bad)?,
                u32_at(&b, 18).ok_or_else(bad)?,
                u32_at(&b, 22).ok_or_else(bad)?,
                26,
            ),
            FrameKind::Jpeg => return Err(bad()),
        };
        if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
            return Err(bad());
        }
        let expected = w as usize * h as usize * 3;
        let packed = b.get(data_at..).ok_or_else(bad)?;
        // The size lz4 claims is checked before anything is made that big.
        let claimed = u32_at(packed, 0).ok_or_else(bad)? as usize;
        if claimed != expected {
            return Err(bad());
        }
        let rgb = lz4_flex::decompress_size_prepended(packed).map_err(|_| bad())?;
        if rgb.len() != expected {
            return Err(bad());
        }
        Ok((x, y, w, h, rgb))
    }

    /// A camera picture's JPEG bytes.
    pub fn jpeg(&self, f: &FrameEntry) -> Result<Vec<u8>, RecordingError> {
        let b = self.payload(f.offset, f.len)?;
        Ok(b.get(10..).unwrap_or_default().to_vec())
    }

    /// Sound of a track: `count` samples per channel from sample number `first`, interleaved,
    /// as it was recorded (silence where there's none).
    pub fn sound(&self, stream: u8, first: u64, count: usize) -> Vec<i16> {
        let channels = self.stream(stream).map(|s| usize::from(s.channels.clamp(1, 2))).unwrap_or(1);
        let mut out = vec![0i16; count.saturating_mul(channels)];
        let Some(list) = self.audio.get(&stream) else { return out };
        let end = first.saturating_add(count as u64);
        let start = list.partition_point(|a| a.first.saturating_add(a.count) <= first);
        for a in list.iter().skip(start).take_while(|a| a.first < end) {
            let Ok(b) = self.payload(a.offset, a.len) else { continue };
            let samples = b.get(9..).unwrap_or_default();
            let from = first.max(a.first);
            let to = end.min(a.first.saturating_add(a.count));
            for n in from..to {
                for c in 0..channels {
                    let src = ((n - a.first) as usize * channels + c) * 2;
                    let dst = (n - first) as usize * channels + c;
                    if let (Some(&[lo, hi]), Some(d)) = (samples.get(src..src + 2), out.get_mut(dst)) {
                        *d = i16::from_le_bytes([lo, hi]);
                    }
                }
            }
        }
        out
    }
}

/// Builds a track's pictures as they come: jumping is from the key picture before; playing
/// on is one change at a time.
pub struct ScreenDecoder {
    stream: u8,
    width: u32,
    height: u32,
    /// The picture so far (RGB) and which entry it is.
    rgb: Vec<u8>,
    at: Option<usize>,
}

impl ScreenDecoder {
    pub fn new(stream: u8) -> ScreenDecoder {
        ScreenDecoder { stream, width: 0, height: 0, rgb: Vec::new(), at: None }
    }

    /// The picture showing at `pts` µs (the last one recorded by then), if there is one.
    pub fn frame_at(&mut self, rec: &Recording, pts: u64) -> Option<Raster> {
        let frames = rec.frames(self.stream);
        let target = frames.partition_point(|f| f.pts <= pts).checked_sub(1)?;
        let key = frames.get(..=target)?.iter().rposition(|f| f.kind == FrameKind::Key)?;
        let from = match self.at {
            Some(a) if a >= key && a <= target => a + 1,
            _ => key,
        };
        for i in from..=target {
            let Some(f) = frames.get(i) else { break };
            match rec.screen_part(f) {
                Ok((x, y, w, h, part)) => {
                    if f.kind == FrameKind::Key {
                        (self.width, self.height, self.rgb) = (w, h, part);
                    } else if x.checked_add(w).is_some_and(|r| r <= self.width) && y.checked_add(h).is_some_and(|b| b <= self.height) {
                        let row = self.width as usize * 3;
                        for (j, src) in part.chunks_exact(w as usize * 3).enumerate() {
                            let start = (y as usize + j) * row + x as usize * 3;
                            if let Some(dst) = self.rgb.get_mut(start..start + src.len()) {
                                dst.copy_from_slice(src);
                            }
                        }
                    }
                }
                Err(e) => log::warn!("{e}"),
            }
            self.at = Some(i);
        }
        if self.rgb.is_empty() {
            return None;
        }
        let rgba: Vec<u8> = self.rgb.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect();
        Raster::new(self.width, self.height, rgba).ok()
    }
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::Color;

    use super::*;

    fn header() -> Header {
        Header {
            version: 1,
            created: "2026-11-02T10:00:00".into(),
            fps: 30,
            streams: vec![
                StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 40, height: 30, channels: 0 },
                StreamInfo { id: 1, kind: StreamKind::Microphone, name: "Mic".into(), width: 0, height: 0, channels: 1 },
            ],
        }
    }

    fn picture(t: u32) -> Raster {
        let mut r = Raster::filled(40, 30, Color::WHITE).unwrap();
        for y in 5..10 {
            for x in t..t + 6 {
                let i = ((y * 40 + x) * 4) as usize;
                r.pixels_mut()[i..i + 3].copy_from_slice(&[200, 0, 0]);
            }
        }
        r
    }

    fn record(n: u32, finish: bool) -> Vec<u8> {
        let mut w = RecordingWriter::new(Vec::new(), &header()).unwrap();
        for t in 0..n {
            w.video_frame(0, u64::from(t) * 100_000, &picture(t)).unwrap();
            w.audio(1, u64::from(t) * 4800, &vec![t as i16; 4800]).unwrap();
        }
        // The same picture again writes nothing.
        let before = w.written();
        w.video_frame(0, u64::from(n) * 100_000, &picture(n - 1)).unwrap();
        assert_eq!(w.written(), before);
        if finish { w.finish(u64::from(n) * 100_000).unwrap() } else { w.out }
    }

    #[test]
    fn pictures_come_back_as_they_were_at_any_moment() {
        let rec = Recording::open(Box::new(record(30, true))).unwrap();
        assert!(rec.finished);
        assert_eq!(rec.duration_us, 3_000_000);
        // A key picture every two seconds, the rest only what changed.
        let kinds: Vec<FrameKind> = rec.frames(0).iter().map(|f| f.kind).collect();
        assert_eq!(kinds.iter().filter(|k| **k == FrameKind::Key).count(), 2);
        let mut d = ScreenDecoder::new(0);
        for t in [0u32, 1, 2, 25, 7, 19, 20, 29] {
            let got = d.frame_at(&rec, u64::from(t) * 100_000 + 50_000).unwrap();
            assert_eq!(got.pixels(), picture(t).pixels(), "picture {t}");
        }
    }

    #[test]
    fn sound_comes_back_where_it_was_with_silence_around_it() {
        let rec = Recording::open(Box::new(record(10, true))).unwrap();
        let s = rec.sound(1, 4800 * 3 - 2, 4);
        assert_eq!(s, vec![2, 2, 3, 3]);
        let past = rec.sound(1, 4800 * 10 - 1, 3);
        assert_eq!(past, vec![9, 0, 0]);
        assert_eq!(rec.stream_duration(1), 1_000_000);
    }

    #[test]
    fn a_recording_cut_off_halfway_opens_with_what_was_written() {
        let mut bytes = record(12, false);
        let whole = bytes.len();
        bytes.truncate(whole - 7);
        let rec = Recording::open(Box::new(bytes)).unwrap();
        assert!(!rec.finished);
        assert!(rec.duration_us >= 1_000_000);
        let mut d = ScreenDecoder::new(0);
        assert_eq!(d.frame_at(&rec, 500_000).unwrap().pixels(), picture(5).pixels());
    }

    #[test]
    fn junk_is_refused_or_read_as_far_as_it_makes_sense() {
        assert!(matches!(Recording::open(Box::new(b"not a video".to_vec())), Err(RecordingError::NotARecording)));
        let good = record(5, true);
        // Every byte after the header flipped in turn: never a panic.
        for i in (40..good.len()).step_by(7) {
            let mut bad = good.clone();
            bad[i] ^= 0xA5;
            if let Ok(rec) = Recording::open(Box::new(bad)) {
                let mut d = ScreenDecoder::new(0);
                let _ = d.frame_at(&rec, 400_000);
                let _ = rec.sound(1, 0, 30_000);
            }
        }
    }

    #[test]
    fn a_damaged_picture_claiming_a_huge_size_is_not_made() {
        let mut w = RecordingWriter::new(Vec::new(), &header()).unwrap();
        w.video_frame(0, 0, &picture(0)).unwrap();
        let mut bytes = w.finish(100_000).unwrap();
        // The lz4 size field of the key picture, set to 4 GB.
        let at = bytes.windows(4).position(|w| w == b"FRAM").unwrap() + 12 + 18;
        bytes[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let rec = Recording::open(Box::new(bytes)).unwrap();
        assert!(ScreenDecoder::new(0).frame_at(&rec, 0).is_none());
    }
}
