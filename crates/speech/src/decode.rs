//! Turning sound into timed lines, 30 seconds at a time: greedy decoding with timestamps, the
//! way OpenAI's `transcribe` does it (the timestamp rules, moving on to the last whole line,
//! leaving out windows that hold no speech).
//!
//! Every loop is bounded: at most [`MAX_TOKENS`] tokens a window, and the windows move forward
//! by at least a second each.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::SpeechError;
use crate::mel::{Mel, RATE, WINDOW};
use crate::model::{Model, Specials};

/// The most tokens written for a window (OpenAI's `sample_len`).
pub const MAX_TOKENS: usize = 224;
/// A window this sure there's no speech, and this unsure of what it wrote, is left out.
const NO_SPEECH: f32 = 0.6;
const LOGPROB: f32 = -1.0;
/// The first timestamp can be no later than a second into the window.
const MAX_INITIAL: u32 = 50;
/// Seconds a timestamp step is.
pub const TIME_STEP: f32 = 0.02;

/// A line, in seconds of the sound it was heard in.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

fn log_softmax(x: &[f32]) -> Vec<f32> {
    let m = x.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if !m.is_finite() {
        return vec![f32::NEG_INFINITY; x.len()];
    }
    let s: f32 = x.iter().map(|v| (v - m).exp()).sum();
    let l = m + s.ln();
    x.iter().map(|v| v - l).collect()
}

fn fill(x: &mut [f32], from: usize, to: usize) {
    let to = to.min(x.len());
    if let Some(s) = x.get_mut(from.min(to)..to) {
        s.fill(f32::NEG_INFINITY);
    }
}

/// What may come next after `seq` (the tokens written so far this window): OpenAI's
/// `SuppressTokens`, `SuppressBlank` and `ApplyTimestampRules`.
pub fn rules(logits: &mut [f32], seq: &[u32], s: &Specials) {
    let ts = s.timestamp_begin as usize;
    let eot = s.eot as usize;
    for t in &s.suppress {
        if let Some(v) = logits.get_mut(*t as usize) {
            *v = f32::NEG_INFINITY;
        }
    }
    if let Some(v) = logits.get_mut(s.no_timestamps as usize) {
        *v = f32::NEG_INFINITY;
    }
    if seq.is_empty() {
        for t in &s.begin_suppress {
            if let Some(v) = logits.get_mut(*t as usize) {
                *v = f32::NEG_INFINITY;
            }
        }
    }
    let is_ts = |t: &u32| *t >= s.timestamp_begin;
    let last_ts = seq.last().is_some_and(is_ts);
    let penultimate_ts = seq.len() < 2 || seq.get(seq.len() - 2).is_some_and(is_ts);
    // Timestamps come in pairs: after one that closes a line, text; after one alone, another.
    if last_ts {
        if penultimate_ts {
            fill(logits, ts, logits.len());
        } else {
            fill(logits, 0, eot);
        }
    }
    // Time never goes back, and a line is never empty.
    if let Some(&last) = seq.iter().rev().find(|t| is_ts(t)) {
        let upto = if last_ts && !penultimate_ts { last } else { last.saturating_add(1) };
        fill(logits, ts, upto as usize);
    }
    if seq.is_empty() {
        // A window starts with a timestamp, no later than a second in.
        fill(logits, 0, ts);
        fill(logits, ts + MAX_INITIAL as usize + 1, logits.len());
    }
    // When a timestamp is likelier than any word, it's a timestamp.
    let lp = log_softmax(logits);
    let (words, times) = lp.split_at(ts.min(lp.len()));
    let m = times.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if m.is_finite() {
        let times_lp = m + times.iter().map(|v| (v - m).exp()).sum::<f32>().ln();
        let best_word = words.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        if times_lp > best_word {
            fill(logits, 0, ts);
        }
    }
}

fn argmax(x: &[f32]) -> Option<u32> {
    let mut best: Option<(usize, f32)> = None;
    for (i, v) in x.iter().enumerate() {
        if v.is_finite() && best.is_none_or(|(_, b)| *v > b) {
            best = Some((i, *v));
        }
    }
    best.and_then(|(i, _)| u32::try_from(i).ok())
}

/// What a window gave: its lines (seconds into the window), and how far to move on (samples).
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub lines: Vec<Line>,
    pub advance: usize,
}

/// The lines in the tokens of a window `content` samples long, and how far to move on.
pub fn lines_of(seq: &[u32], content: usize, m: &dyn Model) -> Window {
    let s = m.specials();
    let is_ts: Vec<bool> = seq.iter().map(|t| *t >= s.timestamp_begin).collect();
    let time = |t: u32| t.saturating_sub(s.timestamp_begin) as f32 * TIME_STEP;
    let single_end = is_ts.len() >= 2 && is_ts.get(is_ts.len() - 2) == Some(&false) && is_ts.last() == Some(&true);
    let mut cuts: Vec<usize> = (1..seq.len()).filter(|&i| is_ts.get(i - 1) == Some(&true) && is_ts.get(i) == Some(&true)).collect();
    let mut lines = Vec::new();
    if cuts.is_empty() {
        let text = m.text(seq);
        let end = seq.iter().rev().find(|t| **t > s.timestamp_begin).map_or(content as f32 / RATE as f32, |t| time(*t));
        if !text.is_empty() {
            lines.push(Line { start: 0.0, end, text });
        }
        return Window { lines, advance: content.max(1) };
    }
    if single_end {
        cuts.push(seq.len());
    }
    let mut last = 0;
    for &cut in &cuts {
        let piece = seq.get(last..cut).unwrap_or_default();
        if let (Some(a), Some(b)) = (piece.first(), piece.last()) {
            let text = m.text(piece);
            if !text.is_empty() {
                lines.push(Line { start: time(*a), end: time(*b).max(time(*a)), text });
            }
        }
        last = cut;
    }
    let advance = if single_end {
        content
    } else {
        // To the end of the last whole line: what's after it is heard again in the next window.
        let at = last.checked_sub(1).and_then(|i| seq.get(i)).map_or(0, |t| t.saturating_sub(s.timestamp_begin) as usize);
        (at * (RATE as f32 * TIME_STEP) as usize).clamp(RATE, content.max(RATE))
    };
    Window { lines, advance }
}

/// Decodes one window (`content` samples of sound in it).
pub fn window(m: &mut dyn Model, mel: &[f32], content: usize, cancel: &AtomicBool) -> Result<Window, SpeechError> {
    m.encode(mel)?;
    let sot = m.specials().sot;
    let eot = m.specials().eot;
    let mut logits = m.step(sot, true)?;
    let no_speech = log_softmax(&logits).get(m.specials().no_speech as usize).map_or(0.0, |v| v.exp());
    let mut seq: Vec<u32> = Vec::with_capacity(MAX_TOKENS);
    let mut sum = 0.0f32;
    for i in 0..MAX_TOKENS {
        if i % 16 == 15 && cancel.load(Ordering::Relaxed) {
            return Err(SpeechError::Cancelled);
        }
        rules(&mut logits, &seq, m.specials());
        let Some(next) = argmax(&logits) else { break };
        sum += log_softmax(&logits).get(next as usize).copied().unwrap_or(f32::NEG_INFINITY).max(-100.0);
        if next == eot {
            break;
        }
        seq.push(next);
        if seq.len() >= MAX_TOKENS {
            break;
        }
        logits = m.step(next, false)?;
    }
    let mean = sum / (seq.len() + 1) as f32;
    if no_speech > NO_SPEECH && mean < LOGPROB {
        return Ok(Window { lines: Vec::new(), advance: content.max(1) });
    }
    Ok(lines_of(&seq, content, m))
}

/// The lines in `total` samples of 16 kHz sound, read 30 seconds at a time by `read(start, len)`.
/// `progress` hears how far it has got (0 to 1).
pub fn transcribe(
    m: &mut dyn Model,
    total: usize,
    read: &mut dyn FnMut(usize, usize) -> Vec<f32>,
    progress: &mut dyn FnMut(f32),
    cancel: &AtomicBool,
) -> Result<Vec<Line>, SpeechError> {
    let mel = Mel::new(m.config().n_mels);
    let mut seek = 0usize;
    let mut out: Vec<Line> = Vec::new();
    // Each window moves on by a second at least: this many windows at most.
    let most = total / RATE + 2;
    for _ in 0..most {
        if seek + RATE / 10 >= total {
            break;
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(SpeechError::Cancelled);
        }
        let content = (total - seek).min(WINDOW);
        let sound = read(seek, content);
        let w = window(m, &mel.window(&sound), content, cancel)?;
        let offset = seek as f32 / RATE as f32;
        for l in w.lines {
            out.push(Line { start: l.start + offset, end: l.end + offset, text: l.text });
        }
        seek = seek.saturating_add(w.advance.max(RATE / 10));
        progress((seek as f32 / total.max(1) as f32).min(1.0));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Config;

    /// A model that writes what it's told to, a token at a time.
    struct Script {
        config: Config,
        specials: Specials,
        /// What each window writes (after the start token); a window past the end writes the end.
        windows: Vec<Vec<u32>>,
        window: usize,
        at: usize,
        heard: usize,
    }

    const TS: u32 = 1000;

    fn script(windows: Vec<Vec<u32>>) -> Script {
        Script {
            config: Config {
                n_mels: 80,
                d_model: 16,
                encoder_heads: 1,
                encoder_layers: 1,
                decoder_heads: 1,
                decoder_layers: 1,
                audio_ctx: 1500,
                text_ctx: 448,
                vocab: TS as usize + 1501,
            },
            specials: Specials {
                eot: 900,
                sot: 901,
                no_speech: 902,
                no_timestamps: 903,
                timestamp_begin: TS,
                suppress: vec![5],
                begin_suppress: vec![900],
            },
            windows,
            window: 0,
            at: 0,
            heard: 0,
        }
    }

    impl Model for Script {
        fn config(&self) -> &Config {
            &self.config
        }
        fn specials(&self) -> &Specials {
            &self.specials
        }
        fn encode(&mut self, mel: &[f32]) -> Result<(), SpeechError> {
            assert_eq!(mel.len(), 80 * 3000);
            self.heard += 1;
            Ok(())
        }
        fn step(&mut self, _token: u32, first: bool) -> Result<Vec<f32>, SpeechError> {
            if first {
                self.window = self.heard - 1;
                self.at = 0;
            }
            let mut logits = vec![0.0f32; self.config.vocab];
            let next = self.windows.get(self.window).and_then(|w| w.get(self.at)).copied().unwrap_or(self.specials.eot);
            logits[next as usize] = 30.0;
            self.at += 1;
            Ok(logits)
        }
        fn text(&self, tokens: &[u32]) -> String {
            tokens.iter().filter(|t| **t < 900).map(|t| format!("w{t} ")).collect::<String>().trim().to_string()
        }
    }

    #[test]
    fn lines_come_between_timestamp_pairs_and_the_window_moves_to_the_last_whole_one() {
        // <0.00> w1 w2 <2.00><2.00> w3 <5.00><5.00> w4 (cut off: no closing timestamp)
        let m = script(vec![]);
        let w = lines_of(&[TS, 1, 2, TS + 100, TS + 100, 3, TS + 250, TS + 250, 4], 480_000, &m);
        assert_eq!(w.lines, vec![Line { start: 0.0, end: 2.0, text: "w1 w2".into() }, Line { start: 2.0, end: 5.0, text: "w3".into() }]);
        assert_eq!(w.advance, 5 * RATE);
        // Ending on a lone timestamp: everything is done, move on by the whole window.
        let w = lines_of(&[TS, 1, TS + 50, TS + 50, 2, TS + 75], 300_000, &m);
        assert_eq!(w.lines.len(), 2);
        assert_eq!(w.lines[1], Line { start: 1.0, end: 1.5, text: "w2".into() });
        assert_eq!(w.advance, 300_000);
        // No pairs at all: one line for the window.
        let w = lines_of(&[TS, 1, 2], 160_000, &m);
        assert_eq!(w.lines, vec![Line { start: 0.0, end: 10.0, text: "w1 w2".into() }]);
    }

    #[test]
    fn the_rules_keep_timestamps_in_pairs_and_moving_forward() {
        let m = script(vec![]);
        let s = m.specials();
        let n = m.config.vocab;
        // At the start: only an early timestamp.
        let mut l = vec![0.0f32; n];
        rules(&mut l, &[], s);
        assert!(l[..TS as usize].iter().all(|v| v.is_infinite()));
        assert!(l[TS as usize + 51..].iter().all(|v| v.is_infinite()));
        assert!(l[TS as usize + 10].is_finite());
        // After a word, a timestamp at least as late as the last one, or a word.
        let mut l = vec![0.0f32; n];
        l[7] = 50.0;
        rules(&mut l, &[TS + 20, 7], s);
        assert!(l[TS as usize + 20].is_infinite() && l[TS as usize + 21].is_finite() && l[7].is_finite());
        assert!(l[5].is_infinite(), "suppressed");
        // After a timestamp closing a line: another, the same, to open the next (or the end, if
        // that is likelier than every timestamp).
        let mut l = vec![0.0f32; n];
        l[900] = 50.0;
        rules(&mut l, &[TS, 7, TS + 30], s);
        assert!(l[7].is_infinite() && l[TS as usize + 30].is_finite() && l[900].is_finite());
        // After a pair: text.
        let mut l = vec![0.0f32; n];
        rules(&mut l, &[TS, 7, TS + 30, TS + 30], s);
        assert!(l[TS as usize..].iter().all(|v| v.is_infinite()) && l[7].is_finite());
    }

    #[test]
    fn a_long_recording_is_heard_window_by_window_and_every_loop_ends() {
        // The first window's last line is cut off: the next starts where the last whole one ended
        // (20 s) and hears the rest again.
        let mut m = script(vec![vec![TS, 1, TS + 1000, TS + 1000, 2], vec![TS, 3, TS + 150]]);
        let total = 35 * RATE;
        let mut reads = Vec::new();
        let mut read = |a: usize, n: usize| {
            reads.push((a, n));
            vec![0.0; n]
        };
        let lines = transcribe(&mut m, total, &mut read, &mut |_| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(reads, vec![(0, WINDOW), (20 * RATE, 15 * RATE)]);
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["w1", "w3"]);
        assert!((lines[1].start - 20.0).abs() < 1e-3 && (lines[1].end - 23.0).abs() < 1e-3);
        // A model that never stops writing: a window is cut at MAX_TOKENS, and the windows still end.
        let mut endless = script(vec![vec![1; 10_000]; 100]);
        let lines = transcribe(&mut endless, 90 * RATE, &mut |_, n| vec![0.0; n], &mut |_| {}, &AtomicBool::new(false)).unwrap();
        assert!(endless.heard <= 3, "{}", endless.heard);
        assert!(lines.len() <= 3);
        // Cancelled: an error, at once.
        let mut m = script(vec![]);
        let r = transcribe(&mut m, 90 * RATE, &mut |_, n| vec![0.0; n], &mut |_| {}, &AtomicBool::new(true));
        assert!(matches!(r, Err(SpeechError::Cancelled)));
    }
}
