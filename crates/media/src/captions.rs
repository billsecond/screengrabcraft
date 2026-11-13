//! Captions of a recording: what was said, when, and by whom. The microphone's words are the
//! owner's ("You"), the computer's sound is the other people's ("Others").
//!
//! The captions are made after the recording, on this computer (`screengrabcraft-speech`), and
//! kept in the recording file in recording time. [`Captions::on_timeline`] places them on the
//! edited video: a line follows its track's clips, and goes where they were cut or muted.
//!
//! Everything read is checked ([`Captions::checked`]): at most [`MAX_CUES`] lines of at most
//! [`MAX_TEXT`] characters, in order, inside the recording.

use serde::{Deserialize, Serialize};

use crate::recording::StreamKind;
use crate::timeline::Timeline;

/// The most lines kept.
pub const MAX_CUES: usize = 100_000;
/// The longest line, in characters.
pub const MAX_TEXT: usize = 400;
/// A line shorter than this on the timeline (what a cut left of it) isn't shown.
const MIN_PIECE_US: u64 = 250_000;

/// Who said a line: the owner (into the microphone) or the people heard through the computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Speaker {
    You,
    Others,
}

impl Speaker {
    pub fn label(self) -> &'static str {
        match self {
            Speaker::You => "You",
            Speaker::Others => "Others",
        }
    }

    /// The track the words were heard on.
    pub fn source(self) -> StreamKind {
        match self {
            Speaker::You => StreamKind::Microphone,
            Speaker::Others => StreamKind::SystemSound,
        }
    }

    /// Whose words a track of this kind holds.
    pub fn of(kind: StreamKind) -> Option<Speaker> {
        match kind {
            StreamKind::Microphone => Some(Speaker::You),
            StreamKind::SystemSound => Some(Speaker::Others),
            StreamKind::Screen | StreamKind::Camera => None,
        }
    }
}

/// One line: from `start` to `end` (µs).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cue {
    pub start: u64,
    pub end: u64,
    pub speaker: Speaker,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Captions {
    /// In order of their start.
    pub cues: Vec<Cue>,
    /// The model that heard them, for the owner.
    #[serde(default)]
    pub model: String,
}

/// Text made safe for a line: no line breaks or control characters, spaces tidied, at most
/// [`MAX_TEXT`] characters.
pub fn clean_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(MAX_TEXT * 4));
    for word in text.split(|c: char| c.is_whitespace() || c.is_control()).filter(|w| !w.is_empty()) {
        if out.chars().count() + word.chars().count() + 1 > MAX_TEXT {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

impl Captions {
    /// Made safe for a recording `duration_us` long: lines inside it, in order, not empty, not
    /// too many and not too long.
    pub fn checked(mut self, duration_us: u64) -> Captions {
        self.cues.truncate(MAX_CUES);
        for c in &mut self.cues {
            c.start = c.start.min(duration_us);
            c.end = c.end.min(duration_us).max(c.start);
            c.text = clean_text(&c.text);
        }
        self.cues.retain(|c| c.end > c.start && !c.text.is_empty());
        self.cues.sort_by_key(|c| (c.start, c.speaker));
        self.model = clean_text(&self.model).chars().take(80).collect();
        self
    }

    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }

    /// The lines as they fall on the edited video: each follows the clips of its speaker's
    /// track, and is left out where that track was cut, deleted or muted.
    pub fn on_timeline(&self, timeline: &Timeline) -> Vec<Cue> {
        let mut out = Vec::new();
        for speaker in [Speaker::You, Speaker::Others] {
            let Some(track) = timeline.find(speaker.source()).and_then(|i| timeline.tracks.get(i)) else { continue };
            if track.muted {
                continue;
            }
            for cue in self.cues.iter().filter(|c| c.speaker == speaker) {
                let whole = cue.end - cue.start;
                for clip in &track.clips {
                    let (from, to) = (cue.start.max(clip.from), cue.end.min(clip.from.saturating_add(clip.len)));
                    if to <= from {
                        continue;
                    }
                    let piece = to - from;
                    if piece < MIN_PIECE_US.min(whole) {
                        continue;
                    }
                    let start = clip.at.saturating_add(from - clip.from);
                    out.push(Cue { start, end: start.saturating_add(piece), speaker, text: cue.text.clone() });
                }
            }
        }
        out.sort_by_key(|c| (c.start, c.speaker));
        out
    }
}

/// The lines showing at `t`: the latest one of each speaker that covers it, the others' first.
pub fn showing(cues: &[Cue], t: u64) -> Vec<&Cue> {
    let mut out: Vec<&Cue> = Vec::with_capacity(2);
    let upto = cues.partition_point(|c| c.start <= t);
    for speaker in [Speaker::Others, Speaker::You] {
        if let Some(c) = cues.get(..upto).unwrap_or_default().iter().rev().take(64).find(|c| c.speaker == speaker && c.end > t) {
            out.push(c);
        }
    }
    out
}

/// `hh:mm:ss,mmm`
fn srt_time(us: u64) -> String {
    let ms = us / 1000;
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000)
}

/// `h:mm:ss`
fn clock(us: u64) -> String {
    let s = us / 1_000_000;
    format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// The lines as SubRip subtitles (`.srt`), each with its speaker: `You: …`, `Others: …`.
pub fn to_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, c) in cues.iter().enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n{}: {}\n\n", i + 1, srt_time(c.start), srt_time(c.end), c.speaker.label(), clean_text(&c.text)));
    }
    out
}

/// The lines as a plain transcript: a paragraph for each turn (the same speaker's lines less
/// than two seconds apart), with when it started.
pub fn to_text(cues: &[Cue]) -> String {
    let mut out = String::new();
    let mut last: Option<(Speaker, u64)> = None;
    for c in cues {
        let text = clean_text(&c.text);
        match last {
            Some((s, end)) if s == c.speaker && c.start <= end.saturating_add(2_000_000) => {
                out.push(' ');
                out.push_str(&text);
            }
            _ => {
                if last.is_some() {
                    out.push('\n');
                }
                out.push_str(&format!("[{}] {}: {}", clock(c.start), c.speaker.label(), text));
            }
        }
        last = Some((c.speaker, last.filter(|(s, _)| *s == c.speaker).map_or(c.end, |(_, e)| e.max(c.end))));
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{CameraLayout, Clip, Track};

    fn cue(start: u64, end: u64, speaker: Speaker, text: &str) -> Cue {
        Cue { start, end, speaker, text: text.to_string() }
    }

    fn timeline() -> Timeline {
        let track = |stream, kind| Track { stream, kind, muted: false, volume: 1.0, clips: vec![Clip { at: 0, from: 0, len: 20_000_000 }] };
        Timeline {
            tracks: vec![track(0, StreamKind::Screen), track(1, StreamKind::SystemSound), track(2, StreamKind::Microphone)],
            camera: CameraLayout::default(),
            captions: Default::default(),
            layers: Vec::new(),
            media: Vec::new(),
        }
    }

    #[test]
    fn lines_follow_their_tracks_cuts_and_go_with_a_mute() {
        let caps = Captions {
            cues: vec![cue(1_000_000, 3_000_000, Speaker::You, "Hello there."), cue(6_000_000, 8_000_000, Speaker::Others, "Hi.")],
            model: String::new(),
        };
        let mut tl = timeline();
        assert_eq!(caps.on_timeline(&tl), caps.cues);
        // Two seconds taken out before the others' line: it comes two seconds sooner.
        tl.cut(4_000_000, 6_000_000).unwrap();
        let on = caps.on_timeline(&tl);
        assert_eq!(on[1], cue(4_000_000, 6_000_000, Speaker::Others, "Hi."));
        // The microphone muted: your line goes.
        tl.set_muted(2, true).unwrap();
        assert_eq!(caps.on_timeline(&tl), vec![cue(4_000_000, 6_000_000, Speaker::Others, "Hi.")]);
        // A cut near the end of a line leaves the part before it; the sliver after is dropped.
        let mut tl = timeline();
        tl.cut(2_000_000, 2_900_000).unwrap();
        let on = caps.on_timeline(&tl);
        assert_eq!(on.iter().filter(|c| c.speaker == Speaker::You).count(), 1, "{on:?}");
    }

    #[test]
    fn what_shows_at_a_moment_is_one_line_a_speaker() {
        let cues =
            vec![cue(0, 2_000_000, Speaker::You, "a"), cue(1_000_000, 4_000_000, Speaker::Others, "b"), cue(1_500_000, 3_000_000, Speaker::You, "c")];
        let at = |t| showing(&cues, t).iter().map(|c| c.text.clone()).collect::<Vec<_>>();
        assert_eq!(at(500_000), ["a"]);
        assert_eq!(at(1_700_000), ["b", "c"]);
        assert_eq!(at(3_500_000), ["b"]);
        assert!(at(4_000_000).is_empty());
    }

    #[test]
    fn srt_and_text_read_as_people_expect() {
        let cues = vec![
            cue(1_500_000, 3_250_000, Speaker::You, "Hello there."),
            cue(3_500_000, 4_000_000, Speaker::You, "How are you?"),
            cue(3_723_004_000, 3_725_000_000, Speaker::Others, "Fine,\nthanks."),
        ];
        assert_eq!(
            to_srt(&cues),
            "1\n00:00:01,500 --> 00:00:03,250\nYou: Hello there.\n\n2\n00:00:03,500 --> 00:00:04,000\nYou: How are you?\n\n\
             3\n01:02:03,004 --> 01:02:05,000\nOthers: Fine, thanks.\n\n"
        );
        assert_eq!(to_text(&cues), "[0:00:01] You: Hello there. How are you?\n[1:02:03] Others: Fine, thanks.\n");
        assert_eq!(to_text(&[]), "");
    }

    #[test]
    fn junk_is_made_safe() {
        let long = "word ".repeat(500);
        let caps = Captions {
            cues: vec![
                cue(5_000_000, 1_000, Speaker::You, "backwards"),
                cue(9_000_000, 99_000_000, Speaker::Others, &long),
                cue(1_000, 2_000, Speaker::You, " \u{7}\r\n "),
                cue(2_000, 3_000, Speaker::You, "fine"),
            ],
            model: "m\n".repeat(100),
        }
        .checked(10_000_000);
        assert_eq!(caps.cues.len(), 2);
        assert_eq!(caps.cues[0].text, "fine");
        assert_eq!(caps.cues[1].end, 10_000_000);
        assert!(caps.cues[1].text.chars().count() <= MAX_TEXT);
        assert!(caps.model.len() <= 80 && !caps.model.contains('\n'));
        let many = Captions { cues: (0..MAX_CUES as u64 + 10).map(|i| cue(i, i + 1, Speaker::You, "x")).collect(), model: String::new() };
        assert_eq!(many.checked(u64::MAX).cues.len(), MAX_CUES);
    }
}
