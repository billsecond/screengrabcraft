//! Hearing what was said in a recording, on this computer: Whisper (OpenAI, MIT) run on the
//! processor by candle, written in Rust. Nothing leaves the computer.
//!
//! Each sound track is heard on its own: the microphone's words are yours, the computer's sound
//! is the other people's ([`Speaker`]). Only the stretches where something is said are heard
//! ([`audio`]); your lines that are the others' heard again through your speakers are left out
//! ([`echo`]). Long lines are cut to subtitle size.
//!
//! Layer L3: no UI and no files. The host reads the model file and hands its bytes over
//! ([`load`]); a recording is heard with [`transcribe`]. Whatever a model file or a recording
//! holds, a bad one is an error, never a crash: everything candle runs is under `catch_unwind`.
//! It takes as many threads as candle is told to (`CANDLE_NUM_THREADS`, `RAYON_NUM_THREADS`):
//! the capture service starts it as a program of its own, at low priority, with one.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod audio;
pub mod decode;
pub mod echo;
pub mod gguf;
pub mod mel;
pub mod model;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::AtomicBool;

use screengrabcraft_media::captions::{Captions, Cue, Speaker, clean_text};
use screengrabcraft_media::recording::Recording;

pub use model::{Model, Whisper};

/// The model installed with ScreenGrabCraft: Whisper base, English only, its large weights in
/// 8 bits.
pub const BUNDLED: &str = "whisper-base.en-q8_0.gguf";
/// The largest model file read.
pub const MAX_MODEL_BYTES: usize = 2 * 1024 * 1024 * 1024;
/// A caption line is cut to about this many characters (two lines of a subtitle).
pub const LINE_CHARS: usize = 84;

#[derive(Debug, thiserror::Error)]
pub enum SpeechError {
    #[error("the speech model: {0}")]
    Model(String),
    #[error("cancelled")]
    Cancelled,
}

/// Runs `f`, turning a panic in it (in candle, on a file it didn't expect) into an error.
fn guarded<T>(what: &str, f: impl FnOnce() -> Result<T, SpeechError>) -> Result<T, SpeechError> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        log::error!("{what} panicked");
        Err(SpeechError::Model(format!("{what} failed unexpectedly")))
    })
}

/// Reads a Whisper model from its file's bytes.
pub fn load(bytes: &[u8]) -> Result<Whisper, SpeechError> {
    if bytes.len() > MAX_MODEL_BYTES {
        return Err(SpeechError::Model("the model file is too large".to_string()));
    }
    guarded("reading the speech model", || Whisper::load(bytes))
}

/// Whether a line is speech, not a sound written down (`[Music]`, `(laughs)`, `♪`), which the
/// model also writes, often for noise.
fn is_speech(text: &str) -> bool {
    let t = text.trim();
    let bracketed = (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')')) || (t.starts_with('*') && t.ends_with('*'));
    !bracketed && t.chars().any(char::is_alphanumeric)
}

/// A line cut into pieces of about [`LINE_CHARS`] characters at most (at a sentence's or a
/// clause's end where it can, else between words), its time shared out by length.
pub fn split_line(start: u64, end: u64, text: &str, speaker: Speaker) -> Vec<Cue> {
    let text = clean_text(text);
    let total = text.chars().count();
    if total <= LINE_CHARS {
        return vec![Cue { start, end, speaker, text }];
    }
    // As many pieces as it takes, about the same length.
    let limit = (total.div_ceil(total.div_ceil(LINE_CHARS)) + 10).min(LINE_CHARS);
    let words: Vec<&str> = text.split(' ').collect();
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, w) in words.iter().enumerate() {
        let len = current.chars().count();
        if len > 0 && len + 1 + w.chars().count() > limit {
            pieces.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(w);
        // A sentence ending past the middle of a piece ends it.
        let ends = w.ends_with(['.', '?', '!']) || (w.ends_with([',', ';', ':']) && current.chars().count() > LINE_CHARS * 2 / 3);
        if ends && current.chars().count() > LINE_CHARS / 2 && i + 1 < words.len() {
            pieces.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    let span = end.saturating_sub(start);
    let mut at = 0usize;
    pieces
        .into_iter()
        .map(|p| {
            let n = p.chars().count() + 1;
            let a = start + span * at as u64 / (total + 1) as u64;
            at += n;
            let b = start + span * at.min(total + 1) as u64 / (total + 1) as u64;
            Cue { start: a, end: b.max(a + 1), speaker, text: p }
        })
        .collect()
}

/// One track heard: its lines in recording time, as caption lines.
fn track(
    model: &mut dyn Model,
    rec: &Recording,
    stream: u8,
    speaker: Speaker,
    levels: &[f32],
    progress: &mut dyn FnMut(f32),
    cancel: &AtomicBool,
) -> Result<Vec<Cue>, SpeechError> {
    let compact = audio::Compact::new(&audio::speech(levels));
    if compact.is_empty() {
        log::info!("{}: nothing said", speaker.label());
        return Ok(Vec::new());
    }
    log::info!("{}: {:.0} s of speech to hear", speaker.label(), compact.len as f32 / mel::RATE as f32);
    let mut read = |start: usize, n: usize| compact.read(rec, stream, start, n);
    let lines = decode::transcribe(model, compact.len, &mut read, progress, cancel)?;
    let mut out: Vec<Cue> = Vec::new();
    for l in lines {
        if !is_speech(&l.text) {
            continue;
        }
        let (start, end) = compact.span(l.start, l.end);
        let end = end.max(start + 300_000);
        // The same line again straight after is the model stuck, not someone repeating it.
        if out.last().is_some_and(|p: &Cue| p.text == clean_text(&l.text) && start <= p.end + 2_000_000) {
            continue;
        }
        out.extend(split_line(start, end, &l.text, speaker));
    }
    Ok(out)
}

/// What was said in a recording: your words (the microphone) and the others' (the computer's
/// sound), each track heard on its own. `progress` hears how far it has got (0 to 1).
pub fn transcribe(model: &mut Whisper, rec: &Recording, progress: &mut dyn FnMut(f32), cancel: &AtomicBool) -> Result<Captions, SpeechError> {
    let name = model.name.clone();
    guarded("hearing the recording", || transcribe_with(model, &name, rec, progress, cancel))
}

/// [`transcribe`] with any model (a test stands one in).
pub fn transcribe_with(
    model: &mut dyn Model,
    name: &str,
    rec: &Recording,
    progress: &mut dyn FnMut(f32),
    cancel: &AtomicBool,
) -> Result<Captions, SpeechError> {
    let tracks: Vec<(Speaker, u8)> = [Speaker::You, Speaker::Others]
        .into_iter()
        .filter_map(|sp| rec.header.streams.iter().find(|s| s.kind == sp.source()).map(|s| (sp, s.id)))
        .collect();
    let levels: Vec<Vec<f32>> = tracks.iter().map(|(_, id)| audio::levels(rec, *id)).collect();
    let level = |sp: Speaker| tracks.iter().position(|(s, _)| *s == sp).and_then(|i| levels.get(i)).map(Vec::as_slice).unwrap_or_default();
    // What the microphone heard from the speakers isn't heard as yours.
    let heard: Vec<Vec<f32>> = tracks
        .iter()
        .zip(&levels)
        .map(
            |((sp, _), l)| {
                if *sp == Speaker::You && !level(Speaker::Others).is_empty() { echo::without_echo(l, level(Speaker::Others)) } else { l.clone() }
            },
        )
        .collect();
    // Each track's share of the work: its speech.
    let weights: Vec<f32> = heard.iter().map(|l| audio::Compact::new(&audio::speech(l)).len as f32).collect();
    let all = weights.iter().sum::<f32>().max(1.0);
    let mut done = 0.0f32;
    let mut cues: Vec<(Speaker, Vec<Cue>)> = Vec::new();
    for (((speaker, stream), l), w) in tracks.iter().zip(&heard).zip(&weights) {
        let share = w / all;
        let mut p = |f: f32| progress(done + share * f);
        let heard = track(model, rec, *stream, *speaker, l, &mut p, cancel)?;
        done += share;
        cues.push((*speaker, heard));
    }
    let others: Vec<Cue> = cues.iter().filter(|(s, _)| *s == Speaker::Others).flat_map(|(_, c)| c.clone()).collect();
    let you: Vec<Cue> = cues.iter().filter(|(s, _)| *s == Speaker::You).flat_map(|(_, c)| c.clone()).collect();
    let mut all_cues = echo::drop_echo(you, &others, level(Speaker::You), level(Speaker::Others));
    all_cues.extend(others);
    progress(1.0);
    Ok(Captions { cues: all_cues, model: name.to_string() }.checked(rec.duration_us))
}

#[cfg(test)]
mod tests;
