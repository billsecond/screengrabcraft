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
    /// A sound file or video brought into the editor and put on the timeline (its lines have no
    /// speaker named).
    Media,
}

impl Speaker {
    pub fn label(self) -> &'static str {
        match self {
            Speaker::You => "You",
            Speaker::Others => "Others",
            Speaker::Media => "",
        }
    }

    /// The track the words were heard on (media's words are heard as the computer's sound is).
    pub fn source(self) -> StreamKind {
        match self {
            Speaker::You => StreamKind::Microphone,
            Speaker::Others | Speaker::Media => StreamKind::SystemSound,
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

/// The lines of the sound files and videos on the timeline's layers, as they fall on the edited
/// video: `of_media` holds each one's lines in its own time (by its id in the media bin); a
/// line follows its item, and is left out where the item was trimmed, or its layer hidden or
/// silenced.
pub fn layer_cues(timeline: &Timeline, of_media: &std::collections::HashMap<u32, Vec<Cue>>) -> Vec<Cue> {
    use crate::layers::What;
    let mut out = Vec::new();
    for l in &timeline.layers {
        if l.hidden || l.volume <= 0.0 {
            continue;
        }
        for item in &l.items {
            let What::Media { id } = &item.what else { continue };
            let Some(cues) = of_media.get(id) else { continue };
            let end_of_item = item.from.saturating_add(item.len);
            for cue in cues.iter().take(MAX_CUES) {
                let whole = cue.end.saturating_sub(cue.start);
                let (from, to) = (cue.start.max(item.from), cue.end.min(end_of_item));
                if to <= from {
                    continue;
                }
                let piece = to - from;
                if piece < MIN_PIECE_US.min(whole) {
                    continue;
                }
                let start = item.at.saturating_add(from - item.from);
                out.push(Cue { start, end: start.saturating_add(piece), speaker: cue.speaker, text: cue.text.clone() });
            }
            if out.len() > MAX_CUES {
                break;
            }
        }
    }
    out.truncate(MAX_CUES);
    out.sort_by_key(|c| (c.start, c.speaker));
    out
}

/// Everything said in the edited video: the recording's lines (if it has captions) and the
/// lines of what was put on its layers, in order.
pub fn edited_cues(captions: Option<&Captions>, timeline: &Timeline, of_media: &std::collections::HashMap<u32, Vec<Cue>>) -> Vec<Cue> {
    let mut out = captions.map(|c| c.on_timeline(timeline)).unwrap_or_default();
    out.extend(layer_cues(timeline, of_media));
    out.sort_by_key(|c| (c.start, c.speaker));
    out
}

/// Lines read from SubRip subtitles (`.srt`): `You:` and `Others:` lines are theirs, any other
/// line is [`Speaker::Media`]'s. What isn't subtitles is left out; at most [`MAX_CUES`] lines.
pub fn from_srt(text: &str) -> Vec<Cue> {
    let time = |s: &str| -> Option<u64> {
        let s = s.trim();
        let (hms, ms) = s.split_once([',', '.']).unwrap_or((s, "0"));
        let mut parts = hms.split(':').map(|p| p.trim().parse::<u64>().ok());
        let (h, m, sec) = (parts.next()??, parts.next()??, parts.next()??);
        if parts.next().is_some() || m >= 60 || sec >= 60 || h > 999 {
            return None;
        }
        let ms: u64 = ms.trim().chars().take(3).collect::<String>().parse().ok()?;
        Some(((h * 3600 + m * 60 + sec) * 1000 + ms) * 1000)
    };
    let mut out = Vec::new();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines().take(MAX_CUES * 8).peekable();
    while lines.peek().is_some() && out.len() < MAX_CUES {
        // A block: its number (optional), its times, its words up to an empty line.
        let mut block: Vec<&str> = Vec::new();
        for l in lines.by_ref() {
            if l.trim().is_empty() {
                if block.is_empty() {
                    continue;
                }
                break;
            }
            if block.len() < 12 {
                block.push(l);
            }
        }
        let Some(at) = block.iter().position(|l| l.contains("-->")) else { continue };
        let Some((a, b)) = block.get(at).and_then(|l| l.split_once("-->")) else { continue };
        // Anything after the end time (a position) is left out.
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (time(a), time(b)) else { continue };
        let words = block.get(at + 1..).unwrap_or_default().join(" ");
        let (speaker, words) = if let Some(w) = words.strip_prefix("You:") {
            (Speaker::You, w)
        } else if let Some(w) = words.strip_prefix("Others:") {
            (Speaker::Others, w)
        } else {
            (Speaker::Media, words.as_str())
        };
        let text = clean_text(words);
        if end > start && !text.is_empty() {
            out.push(Cue { start, end, speaker, text });
        }
    }
    out.sort_by_key(|c| (c.start, c.speaker));
    out
}

/// What goes before a line's words: its speaker's name and a colon, or nothing.
fn said_by(speaker: Speaker) -> String {
    match speaker.label() {
        "" => String::new(),
        name => format!("{name}: "),
    }
}

/// The lines showing at `t`: the latest one of each speaker that covers it, the others' first.
pub fn showing(cues: &[Cue], t: u64) -> Vec<&Cue> {
    let mut out: Vec<&Cue> = Vec::with_capacity(3);
    let upto = cues.partition_point(|c| c.start <= t);
    for speaker in [Speaker::Media, Speaker::Others, Speaker::You] {
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

/// The lines as SubRip subtitles (`.srt`), each with its speaker: `You: …`, `Others: …` (a
/// media file's lines have none).
pub fn to_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, c) in cues.iter().enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n{}{}\n\n", i + 1, srt_time(c.start), srt_time(c.end), said_by(c.speaker), clean_text(&c.text)));
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
                out.push_str(&format!("[{}] {}{}", clock(c.start), said_by(c.speaker), text));
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
            pointer: Default::default(),
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

    #[test]
    fn subtitles_are_read_back_as_they_were_written() {
        let cues = vec![
            cue(1_500_000, 3_250_000, Speaker::You, "Hello there."),
            cue(3_500_000, 4_000_000, Speaker::Others, "Hi."),
            cue(3_723_004_000, 3_725_000_000, Speaker::Media, "From the song."),
        ];
        let srt = to_srt(&cues);
        assert!(srt.contains("\nFrom the song.\n"), "a media line has no speaker: {srt}");
        assert_eq!(from_srt(&srt), cues);
        assert_eq!(to_text(&cues[2..]), "[1:02:03] From the song.\n");
        // Another program's subtitles: a byte-order mark, CRLF, two lines of words, a position.
        let other =
            "\u{feff}1\r\n00:00:01.000 --> 00:00:02,500 X1:40\r\nTwo\r\nlines\r\n\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nOthers: Yes\r\n";
        assert_eq!(from_srt(other), vec![cue(1_000_000, 2_500_000, Speaker::Media, "Two lines"), cue(3_000_000, 4_000_000, Speaker::Others, "Yes")]);
        // Junk is left out, never panicked over.
        for junk in
            ["", "-->", "1\n99:99:99,999 --> 00:00:01,000\nx", "a --> b\n\n\n", "00:00:05,000 --> 00:00:01,000\nbackwards", "\u{0}\u{ffff}-->:::,,"]
        {
            let _ = from_srt(junk);
        }
        assert!(from_srt("00:00:05,000 --> 00:00:01,000\nbackwards").is_empty());
        assert!(from_srt(&"1\n00:00:01,000 --> 00:00:02,000\nx\n\n".repeat(MAX_CUES + 5)).len() <= MAX_CUES);
    }

    #[test]
    fn a_sound_files_lines_follow_its_item_on_the_timeline() {
        use crate::layers::{Item, LayerKind, MediaItem, MediaKind, What};
        let mut tl = timeline();
        let song = MediaItem {
            name: "Song.mp3".into(),
            path: "C:/Music/Song.mp3".into(),
            kind: MediaKind::Sound,
            duration: 30_000_000,
            ..MediaItem::default()
        };
        let id = tl.add_media(song).unwrap();
        // Two seconds into the song, at 5 s on the timeline, four seconds of it.
        let item = Item { len: 4_000_000, from: 2_000_000, what: What::Media { id }, ..Item::default() };
        let (layer, _) = tl.place(item, None, 5_000_000).unwrap();
        assert_eq!(tl.layer(layer).unwrap().kind, LayerKind::Sound);
        let mut of = std::collections::HashMap::new();
        of.insert(
            id,
            vec![
                cue(500_000, 1_500_000, Speaker::Media, "before the part used"),
                cue(3_000_000, 4_000_000, Speaker::Media, "in it"),
                cue(5_500_000, 7_000_000, Speaker::Media, "cut at its end"),
            ],
        );
        let on = layer_cues(&tl, &of);
        assert_eq!(on, vec![cue(6_000_000, 7_000_000, Speaker::Media, "in it"), cue(8_500_000, 9_000_000, Speaker::Media, "cut at its end")]);
        // With the recording's own lines, in order.
        let caps = Captions { cues: vec![cue(6_500_000, 8_000_000, Speaker::You, "And mine.")], model: String::new() };
        let all = edited_cues(Some(&caps), &tl, &of);
        assert_eq!(all.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["in it", "And mine.", "cut at its end"]);
        // Its layer silenced: its lines go.
        tl.set_layer(layer, None, Some(0.0), None).unwrap();
        assert!(layer_cues(&tl, &of).is_empty());
    }
}
