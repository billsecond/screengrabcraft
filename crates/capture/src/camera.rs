//! The camera for a recording, through nokhwa: Media Foundation on Windows, AVFoundation on
//! macOS (Linux and the BSDs: not yet). Pictures are made at most 640 × 480 and kept as JPEGs: the
//! camera sits in a corner of the video, so more would only make the recording bigger.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::CaptureError;

/// The largest camera picture kept.
const MAX_W: u32 = 640;
const MAX_H: u32 = 480;
const QUALITY: u8 = 80;

/// One picture from the camera: when (since the recording started) and the JPEG.
#[derive(Clone, Debug)]
pub struct CameraFrame {
    pub at: Duration,
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// The cameras, by number and name.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn cameras() -> Vec<(u32, String)> {
    allow();
    match nokhwa::query(nokhwa::utils::ApiBackend::Auto) {
        Ok(list) => list
            .iter()
            .filter_map(|c| match c.index() {
                nokhwa::utils::CameraIndex::Index(i) => Some((*i, c.human_name())),
                nokhwa::utils::CameraIndex::String(_) => None,
            })
            .collect(),
        Err(e) => {
            log::warn!("listing cameras: {e}");
            Vec::new()
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn cameras() -> Vec<(u32, String)> {
    Vec::new()
}

/// macOS asks the owner before a program may use the camera; this asks (once) and waits.
#[cfg(target_os = "macos")]
fn allow() {
    if nokhwa::nokhwa_check() {
        return;
    }
    nokhwa::nokhwa_initialize(|_| {});
    let until = Instant::now() + Duration::from_secs(30);
    while !nokhwa::nokhwa_check() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(target_os = "windows")]
fn allow() {}

pub struct CameraCapture {
    pub name: String,
    rx: Receiver<CameraFrame>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// Why the last picture asked for didn't come, as the system said.
    trouble: Arc<std::sync::Mutex<Option<String>>>,
}

/// What to tell the owner about a camera that sends no pictures, from what the system said: a
/// camera another program has to itself is said so plainly.
pub fn trouble_text(said: &str) -> String {
    let s = said.to_ascii_lowercase();
    if s.contains("preempted") || s.contains("0xc00d3ea3") || s.contains("0xc00d3704") || s.contains("in use") {
        "it's in use by another program (a camera app or a video call): close that, then record again".to_string()
    } else {
        said.chars().take(200).collect()
    }
}

impl CameraCapture {
    /// Starts camera `index`; times count from `started`.
    pub fn start(index: u32, started: Instant) -> Result<CameraCapture, CaptureError> {
        let stop = Arc::new(AtomicBool::new(false));
        let trouble = Arc::new(std::sync::Mutex::new(None));
        let (tx, rx) = channel::<CameraFrame>();
        let (ready_tx, ready_rx) = sync_channel::<Result<String, String>>(1);
        let (s, said) = (stop.clone(), trouble.clone());
        let thread = std::thread::Builder::new()
            .name("camera".into())
            .spawn(move || {
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                run(index, started, &tx, &ready_tx, &s, &said);
                #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                {
                    let _ = (index, started, &tx, &s, &said);
                    let _ = ready_tx.send(Err("cameras can't be recorded on this system yet".to_string()));
                }
            })
            .map_err(|e| CaptureError::Device(e.to_string()))?;
        match ready_rx.recv_timeout(Duration::from_secs(40)) {
            Ok(Ok(name)) => Ok(CameraCapture { name, rx, stop, thread: Some(thread), trouble }),
            Ok(Err(e)) => Err(CaptureError::Device(e)),
            Err(_) => {
                stop.store(true, Ordering::SeqCst);
                Err(CaptureError::Device("the camera didn't start".to_string()))
            }
        }
    }

    /// Why no picture came, in plain words, if the system said (see [`trouble_text`]).
    pub fn trouble(&self) -> Option<String> {
        self.trouble.lock().ok().and_then(|t| t.as_deref().map(trouble_text))
    }

    /// The pictures that came in since last time.
    pub fn take(&self) -> Vec<CameraFrame> {
        self.rx.try_iter().collect()
    }

    /// Stops the camera (its light goes off); what came in up to now is returned. A camera stuck
    /// waiting for a picture isn't waited for longer than a moment.
    pub fn stop(mut self) -> Vec<CameraFrame> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            crate::join_within(t, crate::STOP_WAIT, "the camera");
        }
        self.take()
    }
}

impl Drop for CameraCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn run(
    index: u32,
    started: Instant,
    tx: &std::sync::mpsc::Sender<CameraFrame>,
    ready: &std::sync::mpsc::SyncSender<Result<String, String>>,
    stop: &AtomicBool,
    trouble: &std::sync::Mutex<Option<String>>,
) {
    use nokhwa::Camera;
    use nokhwa::pixel_format::RgbFormat;
    use nokhwa::utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution};

    allow();
    let wanted = CameraFormat::new(Resolution::new(1280, 720), FrameFormat::MJPEG, 30);
    let opened = Camera::new(CameraIndex::Index(index), RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(wanted)))
        .or_else(|_| Camera::new(CameraIndex::Index(index), RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate)));
    let mut cam = match opened {
        Ok(c) => c,
        Err(e) => {
            let _ = ready.send(Err(format!("the camera could not be opened: {e}")));
            return;
        }
    };
    if let Err(e) = cam.open_stream() {
        let _ = ready.send(Err(format!("the camera could not be started: {e}")));
        return;
    }
    let _ = ready.send(Ok(cam.info().human_name()));
    let mut failures = 0;
    while !stop.load(Ordering::SeqCst) {
        match cam.frame() {
            Ok(buf) => {
                let at = started.elapsed();
                let res = buf.resolution();
                let format = match buf.source_frame_format() {
                    FrameFormat::MJPEG => Raw::Mjpeg,
                    FrameFormat::YUYV => Raw::Yuyv,
                    FrameFormat::NV12 => Raw::Nv12,
                    FrameFormat::GRAY => Raw::Gray,
                    FrameFormat::RAWRGB => Raw::Rgb,
                    FrameFormat::RAWBGR => Raw::Bgr,
                };
                match to_jpeg(format, buf.buffer(), res.width(), res.height()) {
                    Some((jpeg, width, height)) => {
                        failures = 0;
                        if tx.send(CameraFrame { at, jpeg, width, height }).is_err() {
                            break;
                        }
                    }
                    None => log::warn!("a camera picture could not be read"),
                }
            }
            Err(e) => {
                failures += 1;
                if failures == 1 || failures % 100 == 0 {
                    log::warn!("camera: {e}");
                    if let Ok(mut t) = trouble.lock() {
                        *t = Some(e.to_string());
                    }
                }
                if failures > 300 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    let _ = cam.stop_stream();
}

/// How the camera hands its pictures over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Raw {
    Mjpeg,
    Yuyv,
    Nv12,
    Gray,
    Rgb,
    Bgr,
}

fn clamp8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// BT.601 YUV (as cameras send it) to RGB.
fn yuv_rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
    let (c, d, e) = (i32::from(y) - 16, i32::from(u) - 128, i32::from(v) - 128);
    [clamp8((298 * c + 409 * e + 128) >> 8), clamp8((298 * c - 100 * d - 208 * e + 128) >> 8), clamp8((298 * c + 516 * d + 128) >> 8)]
}

/// A camera picture, made no larger than 640 × 480, as a JPEG with its size. None when the data
/// doesn't fit what the camera said.
pub fn to_jpeg(format: Raw, data: &[u8], width: u32, height: u32) -> Option<(Vec<u8>, u32, u32)> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return None;
    }
    let rgb: image::RgbImage = match format {
        Raw::Mjpeg => image::load_from_memory_with_format(&with_huffman_tables(data), image::ImageFormat::Jpeg).ok()?.to_rgb8(),
        Raw::Yuyv => {
            let px: Vec<u8> = data
                .get(..w * h * 2)?
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|&[y0, u, y1, v]| [yuv_rgb(y0, u, v), yuv_rgb(y1, u, v)])
                .flatten()
                .collect();
            image::RgbImage::from_raw(width, height, px)?
        }
        Raw::Nv12 => {
            let (ys, uv) = data.get(..w * h * 3 / 2)?.split_at(w * h);
            let mut px = Vec::with_capacity(w * h * 3);
            for y in 0..h {
                for x in 0..w {
                    let at = (y / 2) * w + (x / 2) * 2;
                    let (u, v) = (uv.get(at).copied().unwrap_or(128), uv.get(at + 1).copied().unwrap_or(128));
                    px.extend(yuv_rgb(ys.get(y * w + x).copied().unwrap_or(16), u, v));
                }
            }
            image::RgbImage::from_raw(width, height, px)?
        }
        Raw::Gray => image::RgbImage::from_raw(width, height, data.get(..w * h)?.iter().flat_map(|g| [*g, *g, *g]).collect())?,
        Raw::Rgb => image::RgbImage::from_raw(width, height, data.get(..w * h * 3)?.to_vec())?,
        Raw::Bgr => {
            image::RgbImage::from_raw(width, height, data.get(..w * h * 3)?.as_chunks::<3>().0.iter().flat_map(|&[b, g, r]| [r, g, b]).collect())?
        }
    };
    let f = (f64::from(MAX_W) / f64::from(rgb.width())).min(f64::from(MAX_H) / f64::from(rgb.height())).min(1.0);
    let small = if f < 1.0 {
        let (sw, sh) = (((f64::from(rgb.width()) * f) as u32).max(2), ((f64::from(rgb.height()) * f) as u32).max(2));
        image::imageops::resize(&rgb, sw, sh, image::imageops::FilterType::Triangle)
    } else {
        rgb
    };
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, QUALITY).encode_image(&small).ok()?;
    Some((out, small.width(), small.height()))
}

/// Many cameras leave the Huffman tables out of their JPEGs (Motion JPEG takes the standard
/// ones as read). This puts the standard tables (ITU T.81, annex K) in when they're missing.
pub fn with_huffman_tables(jpeg: &[u8]) -> Vec<u8> {
    let mut at = 2usize;
    // Walk the segments up to the start of the picture data.
    while at + 4 <= jpeg.len() {
        let (Some(&ff), Some(&marker)) = (jpeg.get(at), jpeg.get(at + 1)) else { break };
        if ff != 0xFF {
            break;
        }
        match marker {
            0xC4 => return jpeg.to_vec(),
            0xDA => {
                let mut out = Vec::with_capacity(jpeg.len() + DHT.len());
                out.extend_from_slice(jpeg.get(..at).unwrap_or_default());
                out.extend_from_slice(&DHT);
                out.extend_from_slice(jpeg.get(at..).unwrap_or_default());
                return out;
            }
            _ => {
                let len = jpeg.get(at + 2..at + 4).map(|b| usize::from(u16::from_be_bytes([b[0], b[1]]))).unwrap_or(0);
                if len < 2 {
                    break;
                }
                at += 2 + len;
            }
        }
    }
    jpeg.to_vec()
}

/// The standard tables as one DHT segment: DC and AC for luminance, then for chrominance.
const DHT: [u8; 420] = {
    const DC_BITS_L: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
    const DC_BITS_C: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
    const DC_VALS: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    const AC_BITS_L: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7D];
    const AC_VALS_L: [u8; 162] = [
        0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1,
        0x08, 0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26,
        0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56,
        0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85,
        0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA,
        0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6,
        0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9,
        0xFA,
    ];
    const AC_BITS_C: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
    const AC_VALS_C: [u8; 162] = [
        0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71, 0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42,
        0x91, 0xA1, 0xB1, 0xC1, 0x09, 0x23, 0x33, 0x52, 0xF0, 0x15, 0x62, 0x72, 0xD1, 0x0A, 0x16, 0x24, 0x34, 0xE1, 0x25, 0xF1, 0x17, 0x18, 0x19,
        0x1A, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55,
        0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x82, 0x83,
        0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8,
        0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4,
        0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9,
        0xFA,
    ];
    let mut out = [0u8; 420];
    out[0] = 0xFF;
    out[1] = 0xC4;
    // 2 (length) + 4 × (1 + 16) + 12 + 162 + 12 + 162 = 418.
    out[2] = 0x01;
    out[3] = 0xA2;
    let mut at = 4;
    let tables: [(u8, &[u8; 16], &[u8]); 4] =
        [(0x00, &DC_BITS_L, &DC_VALS), (0x10, &AC_BITS_L, &AC_VALS_L), (0x01, &DC_BITS_C, &DC_VALS), (0x11, &AC_BITS_C, &AC_VALS_C)];
    let mut t = 0;
    while t < 4 {
        let (class, bits, vals) = tables[t];
        out[at] = class;
        at += 1;
        let mut i = 0;
        while i < 16 {
            out[at] = bits[i];
            at += 1;
            i += 1;
        }
        let mut i = 0;
        while i < vals.len() {
            out[at] = vals[i];
            at += 1;
            i += 1;
        }
        t += 1;
    }
    out
};

#[cfg(test)]
mod tests {
    use super::*;

    fn test_picture() -> image::RgbImage {
        image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([(x * 4) as u8, (y * 5) as u8, 128]))
    }

    /// A JPEG with its Huffman tables taken out, as many cameras send them.
    fn without_tables(jpeg: &[u8]) -> Vec<u8> {
        let mut out = jpeg[..2].to_vec();
        let mut at = 2;
        loop {
            let marker = jpeg[at + 1];
            if marker == 0xDA {
                out.extend_from_slice(&jpeg[at..]);
                return out;
            }
            let len = usize::from(u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]));
            if marker != 0xC4 {
                out.extend_from_slice(&jpeg[at..at + 2 + len]);
            }
            at += 2 + len;
        }
    }

    #[test]
    fn the_standard_tables_are_whole() {
        // Each table's counts add up to its values.
        let mut at = 4;
        for expected in [12usize, 162, 12, 162] {
            let n: usize = DHT[at + 1..at + 17].iter().map(|b| usize::from(*b)).sum();
            assert_eq!(n, expected);
            at += 17 + n;
        }
        assert_eq!(at, DHT.len());
        assert_eq!(usize::from(u16::from_be_bytes([DHT[2], DHT[3]])), DHT.len() - 2);
    }

    #[test]
    fn a_camera_jpeg_without_tables_reads_the_same_once_they_are_put_back() {
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).encode_image(&test_picture()).unwrap();
        let stripped = without_tables(&jpeg);
        assert!(image::load_from_memory_with_format(&stripped, image::ImageFormat::Jpeg).is_err());
        let fixed = with_huffman_tables(&stripped);
        let a = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        let b = image::load_from_memory(&fixed).unwrap().to_rgb8();
        assert_eq!(a, b);
        // A JPEG that has them is left alone.
        assert_eq!(with_huffman_tables(&jpeg), jpeg);
    }

    #[test]
    fn every_kind_of_camera_picture_becomes_a_small_jpeg() {
        let (w, h) = (1280u32, 720u32);
        let yuyv: Vec<u8> = (0..w * h / 2).flat_map(|_| [120u8, 128, 120, 128]).collect();
        let (jpeg, sw, sh) = to_jpeg(Raw::Yuyv, &yuyv, w, h).unwrap();
        assert_eq!((sw, sh), (640, 360));
        let back = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        assert!(back.pixels().all(|p| (i32::from(p[0]) - 121).abs() < 6));
        let nv12 = vec![128u8; (w * h * 3 / 2) as usize];
        assert!(to_jpeg(Raw::Nv12, &nv12, w, h).is_some());
        assert!(to_jpeg(Raw::Gray, &vec![9u8; 64 * 48], 64, 48).is_some());
        assert!(to_jpeg(Raw::Bgr, &vec![9u8; 64 * 48 * 3], 64, 48).is_some());
    }

    #[test]
    fn short_or_junk_data_is_refused_not_a_panic() {
        assert!(to_jpeg(Raw::Yuyv, &[1, 2, 3], 640, 480).is_none());
        assert!(to_jpeg(Raw::Nv12, &[0; 100], 640, 480).is_none());
        assert!(to_jpeg(Raw::Mjpeg, b"\xFF\xD8junk", 640, 480).is_none());
        assert!(to_jpeg(Raw::Rgb, &[0; 10], 0, 0).is_none());
        assert_eq!(with_huffman_tables(b"\xFF\xD8\xFF"), b"\xFF\xD8\xFF".to_vec());
        assert_eq!(with_huffman_tables(&[]), Vec::<u8>::new());
    }
}
