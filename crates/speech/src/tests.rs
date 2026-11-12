use std::sync::atomic::AtomicBool;

use candle_core::{Device, Tensor};
use screengrabcraft_media::recording::{Header, RecordingWriter, StreamInfo, StreamKind};

use super::*;
use crate::gguf::Value;

/// Numbers that look random, the same every run.
fn noise(seed: u64, n: usize, scale: f32) -> Vec<f32> {
    let mut s = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 2.0 * scale
        })
        .collect()
}

const D: usize = 32;
const VOCAB: usize = 1560;
const TS: u32 = 55;

/// A Whisper model of the real shape but tiny (32 wide, one layer each, 1560 tokens), with
/// made-up weights: it writes nonsense, but every step of the way is the real one.
fn tiny_model() -> Vec<u8> {
    tiny(2)
}

/// [`tiny_model`] with this many attention heads.
fn tiny(heads: u32) -> Vec<u8> {
    let mut tensors: Vec<(String, candle_core::quantized::QTensor)> = Vec::new();
    let mut seed = 1u64;
    let mut add = |name: String, dims: &[usize], scale: f32| {
        seed += 1;
        let n: usize = dims.iter().product();
        let t = Tensor::from_vec(noise(seed, n, scale), dims, &Device::Cpu).unwrap();
        tensors.push((name, model::quantize(&t).unwrap()));
    };
    add("model.encoder.conv1.weight".into(), &[D, 80, 3], 0.1);
    add("model.encoder.conv1.bias".into(), &[D], 0.1);
    add("model.encoder.conv2.weight".into(), &[D, D, 3], 0.1);
    add("model.encoder.conv2.bias".into(), &[D], 0.1);
    add("model.encoder.embed_positions.weight".into(), &[1500, D], 0.1);
    add("model.decoder.embed_tokens.weight".into(), &[VOCAB, D], 0.5);
    add("model.decoder.embed_positions.weight".into(), &[448, D], 0.1);
    for side in ["encoder", "decoder"] {
        let p = format!("model.{side}.layers.0");
        let mut attn = vec!["self_attn"];
        if side == "decoder" {
            attn.push("encoder_attn");
        }
        for a in attn {
            for w in ["q_proj", "k_proj", "v_proj", "out_proj"] {
                add(format!("{p}.{a}.{w}.weight"), &[D, D], 0.2);
                if w != "k_proj" {
                    add(format!("{p}.{a}.{w}.bias"), &[D], 0.1);
                }
            }
            add(format!("{p}.{a}_layer_norm.weight"), &[D], 1.0);
            add(format!("{p}.{a}_layer_norm.bias"), &[D], 0.1);
        }
        add(format!("{p}.fc1.weight"), &[4 * D, D], 0.2);
        add(format!("{p}.fc1.bias"), &[4 * D], 0.1);
        add(format!("{p}.fc2.weight"), &[D, 4 * D], 0.2);
        add(format!("{p}.fc2.bias"), &[D], 0.1);
        add(format!("{p}.final_layer_norm.weight"), &[D], 1.0);
        add(format!("{p}.final_layer_norm.bias"), &[D], 0.1);
        add(format!("model.{side}.layer_norm.weight"), &[D], 1.0);
        add(format!("model.{side}.layer_norm.bias"), &[D], 0.1);
    }
    let words: Vec<Value> = (0..VOCAB).map(|i| Value::String(if i < 50 { format!("Ġw{i}") } else { format!("<|{i}|>") })).collect();
    let n = |v: usize| Value::U32(v as u32);
    let values = vec![
        ("general.architecture", Value::String("whisper".into())),
        ("general.name", Value::String("tiny-test".into())),
        ("whisper.n_mels", n(80)),
        ("whisper.d_model", n(D)),
        ("whisper.encoder.heads", Value::U32(heads)),
        ("whisper.encoder.layers", n(1)),
        ("whisper.decoder.heads", n(2)),
        ("whisper.decoder.layers", n(1)),
        ("whisper.audio_ctx", n(1500)),
        ("whisper.text_ctx", n(448)),
        ("whisper.token.eot", n(50)),
        ("whisper.token.sot", n(51)),
        ("whisper.token.no_speech", n(52)),
        ("whisper.token.no_timestamps", n(53)),
        ("whisper.token.timestamp_begin", n(TS as usize)),
        ("whisper.suppress_tokens", Value::Array(vec![n(3)])),
        ("whisper.begin_suppress_tokens", Value::Array(vec![n(50)])),
        ("tokenizer.ggml.tokens", Value::Array(words)),
    ];
    let refs: Vec<(&str, &candle_core::quantized::QTensor)> = tensors.iter().map(|(n, t)| (n.as_str(), t)).collect();
    gguf::write(&values, &refs).unwrap()
}

/// A recording with a microphone and the computer's sound: bursts of tone on each.
fn recording(secs: u64) -> Recording {
    let streams = vec![
        StreamInfo { id: 1, kind: StreamKind::SystemSound, name: String::new(), width: 0, height: 0, channels: 2 },
        StreamInfo { id: 2, kind: StreamKind::Microphone, name: String::new(), width: 0, height: 0, channels: 1 },
    ];
    let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 30, streams }).unwrap();
    for block in 0..secs * 10 {
        let on = |every: u64| (block / 10) % every == 0;
        let wave = |i: u64, hz: f32| ((block * 4800 + i) as f32 * 2.0 * std::f32::consts::PI * hz / 48_000.0).sin();
        let system: Vec<i16> = (0..4800u64)
            .flat_map(|i| {
                let v = if on(3) { (wave(i, 300.0) * 9000.0) as i16 } else { 0 };
                [v, v]
            })
            .collect();
        let mic: Vec<i16> = (0..4800u64).map(|i| if on(4) { (wave(i, 500.0) * 6000.0) as i16 } else { 0 }).collect();
        w.audio(1, block * 4800, &system).unwrap();
        w.audio(2, block * 4800, &mic).unwrap();
    }
    Recording::open(Box::new(w.finish(secs * 1_000_000).unwrap())).unwrap()
}

#[test]
fn a_tiny_model_hears_a_recording_end_to_end_without_trouble() {
    let bytes = tiny_model();
    let mut m = load(&bytes).unwrap();
    assert_eq!(m.config.d_model, D);
    assert_eq!(m.specials.timestamp_begin, TS);
    assert_eq!(m.text(&[3, 7, 50, 60]), "w3 w7");
    let rec = recording(40);
    let mut last = 0.0f32;
    let caps = transcribe(
        &mut m,
        &rec,
        &mut |p| {
            assert!(p >= last - 1e-6 && p <= 1.0 + 1e-6, "{p} after {last}");
            last = p;
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(caps.model, "tiny-test");
    assert!((last - 1.0).abs() < 1e-6);
    // Whatever nonsense it heard: lines inside the recording, in order, not too long.
    for w in caps.cues.windows(2) {
        assert!(w[0].start <= w[1].start);
    }
    for c in &caps.cues {
        assert!(c.start < c.end && c.end <= 40_000_000 && c.text.chars().count() <= LINE_CHARS + 30, "{c:?}");
    }
    // Asked to stop: it does.
    assert!(matches!(transcribe(&mut m, &rec, &mut |_| {}, &AtomicBool::new(true)), Err(SpeechError::Cancelled)));
}

#[test]
fn a_recording_with_nothing_said_needs_no_model_time() {
    let streams = vec![StreamInfo { id: 2, kind: StreamKind::Microphone, name: String::new(), width: 0, height: 0, channels: 1 }];
    let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 30, streams }).unwrap();
    for block in 0..100u64 {
        w.audio(2, block * 4800, &[3i16; 4800]).unwrap();
    }
    let rec = Recording::open(Box::new(w.finish(10_000_000).unwrap())).unwrap();
    let mut m = load(&tiny_model()).unwrap();
    let started = std::time::Instant::now();
    let caps = transcribe(&mut m, &rec, &mut |_| {}, &AtomicBool::new(false)).unwrap();
    assert!(caps.cues.is_empty());
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn damaged_model_files_are_errors_never_crashes() {
    let good = tiny_model();
    assert!(load(b"").is_err());
    assert!(load(b"not a model at all").is_err());
    // Cut short anywhere.
    for n in [3usize, 4, 8, 23, 24, 200, 4000, good.len() / 3, good.len() - 1] {
        assert!(load(&good[..n]).is_err(), "cut at {n}");
    }
    // Bytes changed here and there in the header and the tensor list.
    for i in (4..6000).step_by(97) {
        let mut bad = good.clone();
        bad[i] ^= 0x5A;
        let _ = load(&bad);
    }
    // Claims of more than there is.
    let mut bad = good.clone();
    bad[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(load(&bad).is_err());
    let mut bad = good.clone();
    bad[16..24].copy_from_slice(&(1u64 << 40).to_le_bytes());
    assert!(load(&bad).is_err());
    // A model of impossible sizes is refused before anything is made of it.
    assert!(load(&tiny(3)).is_err());
    assert!(load(&tiny(0)).is_err());
}

#[test]
fn long_lines_are_cut_to_subtitle_size_and_sounds_are_not_speech() {
    let text = "The design team will review the budget for next year on Tuesday. The night shift should look at the printer \
                on the third floor as soon as possible, and then we can talk again about the backup server.";
    let cues = split_line(10_000_000, 20_000_000, text, Speaker::Others);
    assert!(cues.len() >= 3, "{cues:?}");
    assert!(cues.iter().all(|c| c.text.chars().count() <= LINE_CHARS));
    assert_eq!(cues[0].text, "The design team will review the budget for next year on Tuesday.");
    assert_eq!(cues.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" "), text);
    assert_eq!(cues.first().unwrap().start, 10_000_000);
    assert_eq!(cues.last().unwrap().end, 20_000_000);
    for w in cues.windows(2) {
        assert!(w[0].end <= w[1].start + 1 && w[0].start < w[0].end);
    }
    assert_eq!(split_line(0, 5, "Short.", Speaker::You).len(), 1);
    let one_long_word = "a".repeat(300);
    assert!(split_line(0, 1_000_000, &one_long_word, Speaker::You).iter().all(|c| !c.text.is_empty()));
    for sound in ["[Music]", "(laughs)", "♪ ♪", "...", "*cough*", " [BLANK_AUDIO] "] {
        assert!(!is_speech(sound), "{sound}");
    }
    assert!(is_speech("Thank you."));
}

/// The model installed with ScreenGrabCraft, from the source tree.
fn bundled() -> Option<Vec<u8>> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models").join(BUNDLED);
    std::fs::read(p).ok()
}

#[test]
fn the_installed_model_reads_and_hears_a_tone_without_trouble() {
    let Some(bytes) = bundled() else {
        eprintln!("the speech model isn't in assets/models: skipped");
        return;
    };
    let mut m = load(&bytes).unwrap();
    assert_eq!((m.config.d_model, m.config.encoder_layers, m.config.vocab), (512, 6, 51864));
    assert_eq!((m.specials.eot, m.specials.sot, m.specials.timestamp_begin), (50256, 50257, 50363));
    assert_eq!(m.text(&[15496, 11, 995, 0]), "Hello, world!");
    // Six seconds: a tone on the computer's sound. Whatever it makes of it, it is quick and safe.
    let caps = transcribe(&mut m, &recording(6), &mut |_| {}, &AtomicBool::new(false)).unwrap();
    for c in &caps.cues {
        assert!(c.end <= 6_000_000);
    }
}
