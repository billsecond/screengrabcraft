//! Records a made-up screen (a page with a window moving over it, and typing) and a tone into
//! a recording file, then makes the MP4, timing each: `cargo run --release -p
//! screengrabcraft-media --example export_synthetic -- out-dir [width height secs]`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::time::Instant;

use screengrabcraft_media::export::{ExportSettings, export};
use screengrabcraft_media::recording::{FileSource, Header, Recording, RecordingWriter, StreamInfo, StreamKind};
use screengrabcraft_media::timeline::Timeline;
use screengrabcraft_model::Raster;

fn page(w: usize, h: usize, t: usize) -> Vec<u8> {
    let mut px = vec![255u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let line = y % 24 < 12 && x > 260 && (x / 37 + y / 24) % 5 != 0;
            let typed = (600..612).contains(&y) && (300..300 + t * 6).contains(&x);
            let bx = 300 + (t * 8) % (w - 900);
            let window = (bx..bx + 600).contains(&x) && (200..600).contains(&y) && t % 90 < 45;
            let c = if window {
                [0x4E, 0x3C, 0xA8]
            } else if x < 240 {
                [0x2B, 0x2B, 0x31]
            } else if typed || line {
                [30, 30, 40]
            } else {
                [255, 255, 255]
            };
            px[i..i + 3].copy_from_slice(&c);
        }
    }
    px
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(a.get(1).cloned().unwrap_or_else(|| ".".into()));
    let w: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1920);
    let h: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(1080);
    let secs: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(10);
    let rec_path = dir.join("synthetic.sgvideo");
    let streams = vec![
        StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: w as u32, height: h as u32, channels: 0 },
        StreamInfo { id: 1, kind: StreamKind::SystemSound, name: "Speakers".into(), width: 0, height: 0, channels: 2 },
    ];
    let started = Instant::now();
    let file = std::io::BufWriter::new(std::fs::File::create(&rec_path)?);
    let mut rw = RecordingWriter::new(file, &Header { version: 1, created: String::new(), fps: 30, streams })?;
    let mut grab_ms = 0f64;
    for t in 0..secs * 30 {
        let px = page(w, h, t);
        let r = Raster::new(w as u32, h as u32, px)?;
        let t0 = Instant::now();
        rw.video_frame(0, t as u64 * 1_000_000 / 30, &r)?;
        grab_ms += t0.elapsed().as_secs_f64() * 1000.0;
        if t % 3 == 0 {
            let first = t as u64 / 3 * 4800;
            let pcm: Vec<i16> = (0..4800u64)
                .flat_map(|i| {
                    let v = (((first + i) as f32 * 2.0 * std::f32::consts::PI * 440.0 / 48_000.0).sin() * 6000.0) as i16;
                    [v, v / 2]
                })
                .collect();
            rw.audio(1, first, &pcm)?;
        }
    }
    let size = rw.written();
    rw.finish(secs as u64 * 1_000_000)?;
    println!(
        "recording: {} frames, {:.1} ms a frame to write, {:.1} MB, {:.1} s in all",
        secs * 30,
        grab_ms / (secs * 30) as f64,
        size as f64 / 1e6,
        started.elapsed().as_secs_f64()
    );
    let rec = Arc::new(Recording::open(Box::new(FileSource::open(&rec_path)?))?);
    let tl = Timeline::new(&rec);
    let out = dir.join("synthetic.mp4");
    let t0 = Instant::now();
    let done = export(rec, &tl, &out, ExportSettings::default(), &AtomicU32::new(0), &AtomicBool::new(false))?;
    let took = t0.elapsed().as_secs_f64();
    println!(
        "video: {} x {}, {} frames in {:.1} s = {:.1} fps ({:.1}x real time), {:.2} MB = {:.0} kbit/s",
        done.width,
        done.height,
        done.frames,
        took,
        done.frames as f64 / took,
        secs as f64 / took,
        done.bytes as f64 / 1e6,
        done.bytes as f64 * 8.0 / secs as f64 / 1000.0
    );
    Ok(())
}
