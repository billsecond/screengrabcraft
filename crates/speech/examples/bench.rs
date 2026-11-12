//! Times hearing a recording made of speech files, the way the capture service does it after a
//! Video Production recording, and says how well it heard (the word error rate against what
//! was said).
//!
//! ```sh
//! # One voice on the microphone, cut to `secs` seconds:
//! cargo run --release -p screengrabcraft-speech --example bench -- speech <16 kHz .wav> <what was said .txt> <secs>
//! # A meeting: `turns.txt` lists `NNN-you.wav<TAB>text` and `NNN-others.wav<TAB>text` in order; yours go
//! # on the microphone, theirs on the computer's sound and, faintly and late, on the microphone too
//! # (speakers, not headphones).
//! cargo run --release -p screengrabcraft-speech --example bench -- meeting <folder with turns.txt>
//! ```
//!
//! `BENCH_SAVE=<file.sgvideo>` keeps the recording it made, to try `screengrabcraft --transcribe` on.
//!
//! Its threads are candle's: set `CANDLE_NUM_THREADS` and `RAYON_NUM_THREADS` as the capture
//! service does (1). Use speech that isn't anyone's own: the samples in the pull request were
//! made with Windows' text-to-speech voices.

use std::sync::atomic::AtomicBool;

use screengrabcraft_media::captions::{Captions, Speaker};
use screengrabcraft_media::recording::{Header, Recording, RecordingWriter, StreamInfo, StreamKind};

/// A WAV file's samples at 48 kHz (16-bit, one channel; 16 kHz is made 48 kHz).
fn wav(path: &str) -> Result<Vec<i16>, String> {
    let b = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let (mut at, mut rate, mut data) = (12usize, 0u32, Vec::new());
    while at + 8 <= b.len() {
        let id = b.get(at..at + 4).unwrap_or_default();
        let len = b.get(at + 4..at + 8).and_then(|s| s.try_into().ok()).map(u32::from_le_bytes).unwrap_or(0) as usize;
        let body = b.get(at + 8..(at + 8 + len).min(b.len())).unwrap_or_default();
        if id == b"fmt " {
            rate = body.get(4..8).and_then(|s| s.try_into().ok()).map(u32::from_le_bytes).unwrap_or(0);
        } else if id == b"data" {
            data = body.as_chunks::<2>().0.iter().map(|c| i16::from_le_bytes(*c)).collect();
        }
        at += 8 + len + (len & 1);
    }
    match rate {
        48_000 => Ok(data),
        16_000 => Ok(data
            .windows(2)
            .flat_map(|w| {
                let (a, b) = (i32::from(w[0]), i32::from(w[1]));
                [a, a + (b - a) / 3, a + 2 * (b - a) / 3].map(|v| v as i16)
            })
            .collect()),
        r => Err(format!("{path}: {r} Hz is not read here")),
    }
}

/// A recording of these tracks (microphone, computer's sound), each 48 kHz, one channel.
fn recording(mic: &[i16], system: Option<&[i16]>) -> Result<Recording, String> {
    let mut streams = vec![StreamInfo { id: 2, kind: StreamKind::Microphone, name: "bench".into(), width: 0, height: 0, channels: 1 }];
    if system.is_some() {
        streams.push(StreamInfo { id: 1, kind: StreamKind::SystemSound, name: "bench".into(), width: 0, height: 0, channels: 1 });
    }
    let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 30, streams }).map_err(|e| e.to_string())?;
    let len = mic.len().max(system.map_or(0, <[i16]>::len));
    for (k, start) in (0..len).step_by(4800).enumerate() {
        let block = |s: &[i16]| s.get(start..(start + 4800).min(s.len())).unwrap_or_default().to_vec();
        w.audio(2, (k * 4800) as u64, &block(mic)).map_err(|e| e.to_string())?;
        if let Some(s) = system {
            w.audio(1, (k * 4800) as u64, &block(s)).map_err(|e| e.to_string())?;
        }
    }
    let bytes = w.finish(len as u64 * 125 / 6).map_err(|e| e.to_string())?;
    // `BENCH_SAVE=<file.sgvideo>` keeps the recording, to hear it as the capture service does.
    if let Some(out) = std::env::var_os("BENCH_SAVE") {
        std::fs::write(&out, &bytes).map_err(|e| e.to_string())?;
    }
    Recording::open(Box::new(bytes)).map_err(|e| e.to_string())
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Word error rate of `got` against `want`.
fn wer(want: &str, got: &str) -> f64 {
    let (a, b) = (words(want), words(got));
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, wa) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, wb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(wa != wb)).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev.last().copied().unwrap_or(0) as f64 / a.len().max(1) as f64
}

fn said(c: &Captions, who: Speaker) -> String {
    c.cues.iter().filter(|q| q.speaker == who).map(|q| q.text.as_str()).collect::<Vec<_>>().join(" ")
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let model_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models").join(screengrabcraft_speech::BUNDLED);
    let started = std::time::Instant::now();
    let bytes = std::fs::read(&model_path).map_err(|e| format!("{}: {e}", model_path.display()))?;
    let mut model = screengrabcraft_speech::load(&bytes).map_err(|e| e.to_string())?;
    let loaded = started.elapsed().as_secs_f64();
    let (rec, want_you, want_others) = match args.first().map(String::as_str) {
        Some("speech") => {
            let (Some(w), Some(t), Some(secs)) = (args.get(1), args.get(2), args.get(3).and_then(|s| s.parse::<usize>().ok())) else {
                return Err("usage: bench speech <wav> <txt> <secs>".to_string());
            };
            let mut mic = wav(w)?;
            mic.truncate(secs * 48_000);
            let text = std::fs::read_to_string(t).map_err(|e| e.to_string())?;
            (recording(&mic, None)?, text, String::new())
        }
        Some("meeting") => {
            let dir = std::path::Path::new(args.get(1).ok_or("usage: bench meeting <folder>")?);
            let list = std::fs::read_to_string(dir.join("turns.txt")).map_err(|e| e.to_string())?;
            let (mut mic, mut system) = (Vec::<i16>::new(), Vec::<i16>::new());
            let (mut you, mut others) = (String::new(), String::new());
            for line in list.lines().filter(|l| !l.trim().is_empty()) {
                let (file, text) = line.trim_start_matches('\u{feff}').split_once('\t').ok_or("turns.txt: a line without a tab")?;
                let sound = wav(&dir.join(file).to_string_lossy())?;
                let at = mic.len().max(system.len());
                let pause = 48_000 * 7 / 10;
                mic.resize(at + pause, 0);
                system.resize(at + pause, 0);
                if file.contains("you") {
                    mic.extend(&sound);
                    you.push_str(text);
                    you.push(' ');
                } else {
                    // Theirs: on the computer's sound, and on the microphone 60 ms later, 24 dB down.
                    system.extend(&sound);
                    let late = 48_000 * 6 / 100;
                    mic.resize(at + pause + late, 0);
                    mic.extend(sound.iter().map(|s| s / 16));
                    others.push_str(text);
                    others.push(' ');
                }
            }
            (recording(&mic, Some(&system))?, you, others)
        }
        _ => return Err("usage: bench speech <wav> <txt> <secs> | bench meeting <folder>".to_string()),
    };
    let secs = rec.duration_us as f64 / 1e6;
    let t = std::time::Instant::now();
    let caps = screengrabcraft_speech::transcribe(&mut model, &rec, &mut |_| {}, &AtomicBool::new(false)).map_err(|e| e.to_string())?;
    let took = t.elapsed().as_secs_f64();
    let all = std::env::var_os("BENCH_ALL").is_some();
    for c in caps.cues.iter().take(if all { usize::MAX } else { 8 }) {
        println!("[{:7.2} – {:7.2}] {}: {}", c.start as f64 / 1e6, c.end as f64 / 1e6, c.speaker.label(), c.text);
    }
    let (you, others) = (said(&caps, Speaker::You), said(&caps, Speaker::Others));
    // The reference cut to about as many words as the sound holds (a sample cut short).
    let cut = |want: &str, got: &str| words(want).into_iter().take(words(got).len().max(1)).collect::<Vec<_>>().join(" ");
    println!(
        "RESULT recording={secs:.0}s load={loaded:.2}s heard_in={took:.2}s ({:.3} of real time) lines={} you_wer={:.3} others_wer={}",
        took / secs,
        caps.cues.len(),
        wer(&cut(&want_you, &you), &you),
        if want_others.is_empty() { "-".to_string() } else { format!("{:.3}", wer(&want_others, &others)) },
    );
    if !want_others.is_empty() {
        // Lines of yours that are theirs heard again: each of their words in your lines.
        let theirs = words(&want_others);
        let mine = words(&want_you);
        let echoed = words(&you).iter().filter(|w| theirs.contains(w) && !mine.contains(w)).count();
        println!("ECHO words of theirs among yours: {echoed} (of {} of yours)", words(&you).len());
    }
    Ok(())
}
