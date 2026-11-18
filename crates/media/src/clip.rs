//! A video file brought into the media bin (MP4, M4V, MOV): Windows' own decoders turn its
//! pictures, made smaller, into a temporary file of plain pictures (an AVI of NV12 frames, which
//! Windows writes and this reads), and a picture is read from it when it's shown. Its sound is
//! read apart, by [`crate::media_store::decode_sound`].
//!
//! The file is the owner's, so it's hostile: what Windows can't read is said, sizes are capped
//! (the temporary file stays under [`MAX_BYTES`]), and every number read back is checked.
//!
//! Nothing here is `unsafe`: the windows crate's Windows Runtime calls are safe Rust.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use screengrabcraft_model::Raster;
use windows::Media::MediaProperties::{ContainerEncodingProperties, MediaEncodingProfile, MediaEncodingSubtypes, VideoEncodingProperties};
use windows::Media::Transcoding::MediaTranscoder;
use windows::Storage::StorageFile;
use windows::core::HSTRING;

/// The temporary file of pictures stays under this.
pub const MAX_BYTES: u64 = 1_500_000_000;
/// The longest video brought in (minutes).
pub const MAX_MINS: u64 = 30;

/// A video file's pictures, ready to be shown.
pub struct Clip {
    /// The temporary file (it goes when this does).
    temp: PathBuf,
    file: Mutex<File>,
    /// Where each picture is in it, and how long (0: the one before again).
    frames: Vec<(u64, u32)>,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// How long it is (µs).
    pub duration: u64,
    /// The last picture read, and which.
    last: Mutex<Option<(usize, Arc<Raster>)>>,
}

impl Drop for Clip {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.temp);
    }
}

fn err(e: windows::core::Error) -> String {
    e.message().to_string()
}

fn windows_path(p: &Path) -> Result<HSTRING, String> {
    let whole = std::path::absolute(p).map_err(|e| e.to_string())?;
    Ok(HSTRING::from(whole.to_string_lossy().replace('/', "\\")))
}

/// The pictures' size and rate for a video `w` × `h` of `secs`: at most 640 × 360 at 15 a
/// second, smaller for a long one so the temporary file stays under [`MAX_BYTES`]. Even sizes.
fn plan(w: u32, h: u32, secs: u64) -> (u32, u32, u32) {
    let (w, h) = (w.max(2), h.max(2));
    for (box_w, box_h, fps) in [(640u32, 360u32, 15u32), (480, 270, 15), (480, 270, 10), (320, 180, 10), (240, 136, 8)] {
        let f = (box_w as f32 / w as f32).min(box_h as f32 / h as f32).min(1.0);
        let (ow, oh) = ((((w as f32 * f) as u32) & !1).max(2), (((h as f32 * f) as u32) & !1).max(2));
        let bytes = u64::from(ow) * u64::from(oh) * 3 / 2 * u64::from(fps) * secs.max(1);
        if bytes <= MAX_BYTES {
            return (ow, oh, fps);
        }
    }
    (240, 136, 8)
}

impl Clip {
    /// Reads a video file's pictures into a temporary file in `temp_dir` (Windows decodes it, as
    /// fast as it can). `secs` is how long it's said to be (to size the pictures), if known.
    pub fn open(src: &Path, temp_dir: &Path, secs: Option<u64>) -> Result<Clip, String> {
        let file = StorageFile::GetFileFromPathAsync(&windows_path(src)?).map_err(err)?.join().map_err(err)?;
        let probed =
            MediaEncodingProfile::CreateFromFileAsync(&file).map_err(err)?.join().map_err(|_| "Windows can't read this video".to_string())?;
        let video = probed.Video().map_err(|_| "it has no pictures".to_string())?;
        let (w, h) = (video.Width().map_err(err)?, video.Height().map_err(err)?);
        if w == 0 || h == 0 {
            return Err("it has no pictures".to_string());
        }
        // How long it is, as Windows says (in tenths of a microsecond), else as the caller does.
        let said = file
            .Properties()
            .and_then(|p| p.GetVideoPropertiesAsync())
            .and_then(|o| o.join())
            .and_then(|v| v.Duration())
            .ok()
            .map(|d| d.Duration.max(0) as u64 / 10_000_000)
            .filter(|s| *s > 0);
        let secs = said.or(secs).unwrap_or(MAX_MINS * 60);
        if secs > MAX_MINS * 60 {
            return Err(format!("it's longer than {MAX_MINS} minutes, the longest video that can be brought in"));
        }
        let (ow, oh, fps) = plan(w, h, secs);

        std::fs::create_dir_all(temp_dir).map_err(|e| e.to_string())?;
        let temp = temp_dir.join(format!("clip-{}-{}.avi", std::process::id(), next_number()));
        File::create(&temp).map_err(|e| e.to_string())?;
        // From here on the file goes if anything goes wrong.
        let made = (|| -> Result<(), String> {
            let out = StorageFile::GetFileFromPathAsync(&windows_path(&temp)?).map_err(err)?.join().map_err(err)?;
            let profile = MediaEncodingProfile::new().map_err(err)?;
            let container = ContainerEncodingProperties::new().map_err(err)?;
            container.SetSubtype(&HSTRING::from("AVI")).map_err(err)?;
            profile.SetContainer(&container).map_err(err)?;
            let v = VideoEncodingProperties::CreateUncompressed(&MediaEncodingSubtypes::Nv12().map_err(err)?, ow, oh).map_err(err)?;
            let rate = v.FrameRate().map_err(err)?;
            rate.SetNumerator(fps).map_err(err)?;
            rate.SetDenominator(1).map_err(err)?;
            let aspect = v.PixelAspectRatio().map_err(err)?;
            aspect.SetNumerator(1).map_err(err)?;
            aspect.SetDenominator(1).map_err(err)?;
            profile.SetVideo(&v).map_err(err)?;
            profile.SetAudio(None::<&windows::Media::MediaProperties::AudioEncodingProperties>).map_err(err)?;
            let transcoder = MediaTranscoder::new().map_err(err)?;
            let prepared = transcoder.PrepareFileTranscodeAsync(&file, &out, &profile).map_err(err)?.join().map_err(err)?;
            if !prepared.CanTranscode().map_err(err)? {
                let why = prepared.FailureReason().map_err(err)?;
                return Err(format!("Windows can't read this video's pictures ({why:?})"));
            }
            prepared.TranscodeAsync().map_err(err)?.join().map_err(err)
        })();
        let read = made.and_then(|()| read_avi(&temp));
        match read {
            Ok((frames, aw, ah)) => {
                let (aw, ah) = (if aw > 0 { aw } else { ow }, if ah > 0 { ah } else { oh });
                if frames.is_empty() {
                    let _ = std::fs::remove_file(&temp);
                    return Err("no pictures came out of it".to_string());
                }
                let f = File::open(&temp).map_err(|e| e.to_string())?;
                let duration = frames.len() as u64 * 1_000_000 / u64::from(fps.max(1));
                Ok(Clip { temp, file: Mutex::new(f), frames, width: aw, height: ah, fps, duration, last: Mutex::new(None) })
            }
            Err(e) => {
                let _ = std::fs::remove_file(&temp);
                Err(e)
            }
        }
    }

    /// The picture shown at `t` (µs from its start); its last one after its end.
    pub fn frame_at(&self, t: u64) -> Option<Arc<Raster>> {
        let n = self.frames.len();
        let mut k = (t.saturating_mul(u64::from(self.fps)) / 1_000_000).min(n.saturating_sub(1) as u64) as usize;
        // A frame of no length is the one before it again.
        while k > 0 && self.frames.get(k).is_some_and(|f| f.1 == 0) {
            k -= 1;
        }
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((at, p)) = last.as_ref()
            && *at == k
        {
            return Some(p.clone());
        }
        let &(off, len) = self.frames.get(k)?;
        let want = (self.width as usize).checked_mul(self.height as usize)?.checked_mul(3)? / 2;
        if (len as usize) < want {
            return None;
        }
        let mut buf = vec![0u8; want];
        {
            let mut f = self.file.lock().unwrap_or_else(PoisonError::into_inner);
            f.seek(SeekFrom::Start(off)).ok()?;
            f.read_exact(&mut buf).ok()?;
        }
        let pic = Arc::new(nv12_to_raster(&buf, self.width, self.height)?);
        *last = Some((k, pic.clone()));
        Some(pic)
    }
}

fn next_number() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

/// NV12 (a byte of brightness a pixel, then a pair of colour bytes for each 2 × 2) to RGBA, by
/// BT.601's video range.
pub fn nv12_to_raster(b: &[u8], w: u32, h: u32) -> Option<Raster> {
    let (wu, hu) = (w as usize, h as usize);
    let luma = b.get(..wu.checked_mul(hu)?)?;
    let chroma = b.get(wu * hu..)?;
    let mut px = Vec::with_capacity(wu * hu * 4);
    for y in 0..hu {
        for x in 0..wu {
            let yy = f32::from(*luma.get(y * wu + x)?) - 16.0;
            let c = (y / 2) * wu + (x & !1);
            let u = f32::from(*chroma.get(c).unwrap_or(&128)) - 128.0;
            let v = f32::from(*chroma.get(c + 1).unwrap_or(&128)) - 128.0;
            let r = 1.164 * yy + 1.596 * v;
            let g = 1.164 * yy - 0.392 * u - 0.813 * v;
            let bl = 1.164 * yy + 2.017 * u;
            px.extend_from_slice(&[r.clamp(0.0, 255.0) as u8, g.clamp(0.0, 255.0) as u8, bl.clamp(0.0, 255.0) as u8, 255]);
        }
    }
    Raster::new(w, h, px).ok()
}

/// The pictures in an AVI file: where each is and how long, and their size (from its header, 0
/// if it says none). Reads the RIFF chunks, the OpenDML extensions' too.
fn read_avi(path: &Path) -> Result<(Vec<(u64, u32)>, u32, u32), String> {
    let mut f = std::io::BufReader::new(File::open(path).map_err(|e| e.to_string())?);
    let end = f.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    let mut frames = Vec::new();
    let (mut w, mut h) = (0u32, 0u32);
    // (where a run of chunks starts, where it ends), innermost last.
    let mut runs: Vec<(u64, u64)> = vec![(0, end)];
    let mut steps = 0u32;
    while let Some((at, stop)) = runs.pop() {
        let mut at = at;
        while at.saturating_add(8) <= stop {
            steps += 1;
            if steps > 5_000_000 {
                return Err("the pictures' file is too long".to_string());
            }
            f.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
            let mut head = [0u8; 12];
            if f.read_exact(&mut head[..8]).is_err() {
                break;
            }
            let id = [head[0], head[1], head[2], head[3]];
            let size = u64::from(u32::from_le_bytes([head[4], head[5], head[6], head[7]]));
            let body = at + 8;
            let next = body.saturating_add(size).saturating_add(size & 1).min(stop);
            match &id {
                b"RIFF" | b"LIST" => {
                    // A list of chunks after its kind: gone into, the rest of this run after it.
                    if f.read_exact(&mut head[8..12]).is_ok() {
                        runs.push((next, stop));
                        runs.push((body + 4, body.saturating_add(size).min(stop)));
                        break;
                    }
                }
                b"strf" if w == 0 => {
                    let mut bi = [0u8; 12];
                    if size >= 12 && f.read_exact(&mut bi).is_ok() {
                        w = i32::from_le_bytes([bi[4], bi[5], bi[6], bi[7]]).unsigned_abs();
                        h = i32::from_le_bytes([bi[8], bi[9], bi[10], bi[11]]).unsigned_abs();
                    }
                }
                [_, _, b'd', b'b' | b'c']
                    if id[0].is_ascii_digit() && id[1].is_ascii_digit() && size <= u64::from(u32::MAX) && frames.len() < 2_000_000 =>
                {
                    frames.push((body, size as u32));
                }
                _ => {}
            }
            at = next;
        }
    }
    // Runs are walked inner first; pictures are in the file's order.
    frames.sort_by_key(|f| f.0);
    Ok((frames, w.min(8192), h.min(8192)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_videos_get_smaller_pictures() {
        assert_eq!(plan(1920, 1080, 60), (640, 360, 15));
        assert_eq!(plan(100, 60, 60), (100, 60, 15), "never made larger");
        let (w, h, fps) = plan(1920, 1080, 30 * 60);
        assert!(u64::from(w) * u64::from(h) * 3 / 2 * u64::from(fps) * 1800 <= MAX_BYTES);
        let (w, h, _) = plan(1080, 1920, 10);
        assert!(w % 2 == 0 && h % 2 == 0 && h == 360 && w < h, "upright stays upright");
    }

    #[test]
    fn nv12_grey_is_grey() {
        let (w, h) = (4u32, 2u32);
        let mut b = vec![126u8; 8];
        b.extend_from_slice(&[128; 4]);
        let r = nv12_to_raster(&b, w, h).unwrap();
        let p = r.pixel(3, 1).unwrap();
        assert!(p.r == p.g && p.g == p.b && (120..=135).contains(&p.r), "{p:?}");
        assert!(nv12_to_raster(&b[..5], w, h).is_none(), "too little is none");
    }

    #[test]
    fn junk_avi_is_said_not_panicked_over() {
        let dir = std::env::temp_dir().join(format!("sgc-avi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (i, junk) in
            [Vec::new(), b"RIFF\xff\xff\xff\xffAVI LIST".to_vec(), vec![0xAB; 3000], b"RIFF\x04\x00\x00\x00AVI 00db\xff\xff\xff\x7f".to_vec()]
                .iter()
                .enumerate()
        {
            let p = dir.join(format!("{i}.avi"));
            std::fs::write(&p, junk).unwrap();
            let _ = read_avi(&p);
        }
        assert!(Clip::open(&dir.join("0.avi"), &dir, None).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An MP4 made by Windows' own encoders comes back as pictures.
    #[test]
    fn an_mp4_comes_back_as_pictures() {
        use std::sync::atomic::{AtomicBool, AtomicU32};

        use screengrabcraft_model::Color;

        use crate::export::{Codec, ExportSettings, export};
        use crate::recording::{Header, Recording, RecordingWriter, StreamInfo, StreamKind};
        use crate::timeline::Timeline;

        let streams = vec![StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 320, height: 180, channels: 0 }];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        for t in 0..30u64 {
            let mut pic = Raster::filled(320, 180, Color::WHITE).unwrap();
            // A purple block moving right.
            let x0 = (t * 6) as u32;
            for y in 60..120 {
                for x in x0..x0 + 60 {
                    let i = ((y * 320 + x) * 4) as usize;
                    pic.pixels_mut()[i..i + 3].copy_from_slice(&[0x4E, 0x3C, 0xA8]);
                }
            }
            w.video_frame(0, t * 100_000, &pic).unwrap();
        }
        let rec = Arc::new(Recording::open(Box::new(w.finish(3_000_000).unwrap())).unwrap());
        let tl = Timeline::new(&rec);
        let dir = std::env::temp_dir().join(format!("sgc-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mp4 = dir.join("clip.mp4");
        let s = ExportSettings { fps: 10, codec: Codec::H264, ..ExportSettings::default() };
        export(rec, &tl, &mp4, s, &AtomicU32::new(0), &AtomicBool::new(false)).unwrap();

        let clip = Clip::open(&mp4, &dir, Some(3)).unwrap();
        assert_eq!((clip.width, clip.height), (320, 180));
        assert!((2_500_000..=3_500_000).contains(&clip.duration), "{}", clip.duration);
        let first = clip.frame_at(0).unwrap();
        let p = first.pixel(30, 90).unwrap();
        assert!(p.b > p.g + 40 && p.r < 140, "the purple block at the start: {p:?}");
        let white = first.pixel(250, 20).unwrap();
        assert!(white.r > 220 && white.g > 220 && white.b > 220, "{white:?}");
        let later = clip.frame_at(2_000_000).unwrap();
        let q = later.pixel(30, 90).unwrap();
        assert!(q.r > 200, "the block has moved on: {q:?}");
        let temp = clip.temp.clone();
        drop(clip);
        assert!(!temp.exists(), "its temporary file goes with it");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
