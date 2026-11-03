//! Makes a short made-up video (a box moving over a page, a beep each second) to check that
//! players open what ScreenGrabCraft makes: `cargo run --release -p screengrabcraft-media
//! --example test_video -- out.mp4`.

use std::io::BufWriter;

use screengrabcraft_media::av1::{Av1Encoder, Av1Settings};
use screengrabcraft_media::mp4::{AudioTrack, Mp4Writer, Sample, VideoTrack};
use screengrabcraft_media::opus::{self, OpusEncoder};
use screengrabcraft_media::yuv::I420;

fn page(w: usize, h: usize, t: usize) -> Vec<u8> {
    let mut px = vec![255u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let line = y % 24 < 12 && x > 140 && (x / 37 + y / 24) % 5 != 0;
            let side = x < 120;
            let bx = 200 + (t * 9) % (w - 400);
            let block = (bx..bx + 220).contains(&x) && (180..380).contains(&y);
            let c = if block {
                [0x4E, 0x3C, 0xA8]
            } else if side {
                [0x2B, 0x2B, 0x31]
            } else if line {
                [40, 40, 50]
            } else {
                [255, 255, 255]
            };
            px[i..i + 3].copy_from_slice(&c);
        }
    }
    px
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "test.mp4".to_string());
    let (w, h, fps, secs) = (1280usize, 720usize, 30u32, 6usize);
    let mut enc = Av1Encoder::new(Av1Settings { width: w as u32, height: h as u32, fps, quantizer: 120, speed: 10, threads: 0 })?;
    let video = VideoTrack { width: w as u32, height: h as u32, timescale: fps, av1c: Vec::new() };
    let file = BufWriter::new(std::fs::File::create(&out)?);
    let mut mp4 = Mp4Writer::new(file, Some(video), Some(AudioTrack { channels: 2, pre_skip: opus::PRE_SKIP }))?;
    let mut sound = OpusEncoder::new(2, 96_000)?;
    let started = std::time::Instant::now();
    for s in 0..secs {
        let mut coded = Vec::new();
        for f in 0..fps as usize {
            let px = page(w, h, s * fps as usize + f);
            let frame = I420::from_rgba(&px, w as u32, h as u32).ok_or("frame")?;
            coded.extend(enc.send(&frame)?);
        }
        if s + 1 == secs {
            coded.extend(enc.finish()?);
        }
        if s == 0
            && let Some(first) = coded.first()
        {
            mp4.set_av1c(enc.config_record(first));
        }
        let samples: Vec<Sample> = coded.into_iter().map(|c| Sample { data: c.data, duration: 1, sync: c.key }).collect();
        mp4.write_video(&samples)?;
        // A beep at the start of each second on the left, a low hum on the right.
        let pcm: Vec<f32> = (0..opus::RATE as usize)
            .flat_map(|i| {
                let t = i as f32 / opus::RATE as f32;
                let left = if t < 0.15 { (t * 2.0 * std::f32::consts::PI * 880.0).sin() * 0.4 } else { 0.0 };
                let right = (t * 2.0 * std::f32::consts::PI * 220.0).sin() * 0.1;
                [left, right]
            })
            .collect();
        let mut packets = sound.push(&pcm)?;
        if s + 1 == secs {
            packets.extend(sound.finish()?);
        }
        let samples: Vec<Sample> = packets.into_iter().map(|p| Sample { data: p, duration: opus::FRAME as u32, sync: true }).collect();
        mp4.write_audio(&samples)?;
    }
    mp4.finish()?;
    println!("{out}: {} bytes, {:.1} s to make", std::fs::metadata(&out)?.len(), started.elapsed().as_secs_f64());
    Ok(())
}
