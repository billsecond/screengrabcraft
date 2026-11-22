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
        self.on_timeline_from(timeline).into_iter().map(|(c, _)| c).collect()
    }

    /// [`Captions::on_timeline`], each line with which of the recording's lines it is a piece
    /// of (its place in `cues`).
    pub fn on_timeline_from(&self, timeline: &Timeline) -> Vec<(Cue, usize)> {
        let mut out = Vec::new();
        for speaker in [Speaker::You, Speaker::Others] {
            let Some(track) = timeline.find(speaker.source()).and_then(|i| timeline.tracks.get(i)) else { continue };
            if track.muted {
                continue;
            }
            for (k, cue) in self.cues.iter().enumerate().filter(|(_, c)| c.speaker == speaker) {
                let whole = cue.end - cue.start;
                // Words played backwards say nothing: no line there.
                for clip in track.clips.iter().filter(|c| !c.reverse) {
                    let (from, to) = (cue.start.max(clip.from), cue.end.min(clip.from.saturating_add(clip.source_len())));
                    if to <= from {
                        continue;
                    }
                    if to - from < MIN_PIECE_US.min(whole) {
                        continue;
                    }
                    // Sped up or slowed down with its clip.
                    let start = clip.at.saturating_add(clip.played(from - clip.from));
                    let end = clip.at.saturating_add(clip.played(to - clip.from)).min(clip.end()).max(start.saturating_add(1));
                    out.push((Cue { start, end, speaker, text: cue.text.clone() }, k));
                }
            }
        }
        out.sort_by_key(|(c, _)| (c.start, c.speaker));
        out
    }
}

/// The lines of the sound files and videos on the timeline's layers, as they fall on the edited
/// video: `of_media` holds each one's lines in its own time (by its id in the media bin); a
/// line follows its item, and is left out where the item was trimmed, or its layer hidden or
/// silenced.
pub fn layer_cues(timeline: &Timeline, of_media: &std::collections::HashMap<u32, Vec<Cue>>) -> Vec<Cue> {
    layer_lines(timeline, of_media).into_iter().map(|(c, _)| c).collect()
}

/// The lines of what was put on the layers, each with the layer it's on.
fn layer_lines(timeline: &Timeline, of_media: &std::collections::HashMap<u32, Vec<Cue>>) -> Vec<(Cue, u32)> {
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
                out.push((Cue { start, end: start.saturating_add(piece), speaker: cue.speaker, text: cue.text.clone() }, l.id));
            }
            if out.len() > MAX_CUES {
                break;
            }
        }
    }
    out.truncate(MAX_CUES);
    out.sort_by_key(|(c, l)| (c.start, c.speaker, *l));
    out
}

/// Who said a line of the transcript: one of the recording's speakers (You on its microphone,
/// Others on the computer's sound), or what was put on a layer (by the layer's id).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Who {
    Speaker(Speaker),
    Layer(u32),
}

/// Everything said in the edited video, in order, each line with who said it.
pub fn transcript(captions: Option<&Captions>, timeline: &Timeline, of_media: &std::collections::HashMap<u32, Vec<Cue>>) -> Vec<(Cue, Who)> {
    transcript_from(captions, timeline, of_media).into_iter().map(|(c, w, _)| (c, w)).collect()
}

/// [`transcript`], each of the recording's lines with which of its captions it is a piece of
/// (its place in `cues`, to put it right); a layer's lines with none.
pub fn transcript_from(
    captions: Option<&Captions>,
    timeline: &Timeline,
    of_media: &std::collections::HashMap<u32, Vec<Cue>>,
) -> Vec<(Cue, Who, Option<usize>)> {
    let mut out: Vec<(Cue, Who, Option<usize>)> = captions
        .map(|c| c.on_timeline_from(timeline))
        .unwrap_or_default()
        .into_iter()
        .map(|(c, k)| {
            let who = Who::Speaker(c.speaker);
            (c, who, Some(k))
        })
        .collect();
    out.extend(layer_lines(timeline, of_media).into_iter().map(|(c, l)| (c, Who::Layer(l), None)));
    out.sort_by_key(|(c, w, _)| (c.start, *w));
    out.truncate(MAX_CUES);
    out
}

/// The recording's captions as they are now: as put right by hand (the timeline's
/// [`Timeline::corrected`]), else as heard.
pub fn as_corrected(heard: Option<&Captions>, timeline: &Timeline) -> Option<Captions> {
    match &timeline.corrected {
        Some(cues) => Some(Captions { cues: cues.clone(), model: heard.map(|h| h.model.clone()).unwrap_or_default() }),
        None => heard.cloned(),
    }
}

/// What the transcript calls who said a line: the name given to the track it was heard on (the
/// microphone's for You, the computer's sound's for Others), else You or Others; a layer's name.
pub fn speaker_name(timeline: &Timeline, who: Who) -> String {
    match who {
        Who::Speaker(s) => timeline
            .find(s.source())
            .and_then(|i| timeline.tracks.get(i))
            .and_then(|t| t.name.clone())
            .unwrap_or_else(|| if s == Speaker::Media { "Media".to_string() } else { s.label().to_string() }),
        Who::Layer(id) => timeline.layer(id).map(|l| l.name.clone()).unwrap_or_else(|| "Media".to_string()),
    }
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
/// (WebVTT is read too: see [`from_subtitles`].)
pub fn from_srt(text: &str) -> Vec<Cue> {
    from_subtitles(text)
}

/// A time in subtitles (µs): `hh:mm:ss,mmm` (SubRip), `hh:mm:ss.mmm` or `mm:ss.mmm` (WebVTT).
fn subtitle_time(s: &str) -> Option<u64> {
    let s = s.trim();
    let (hms, frac) = s.split_once([',', '.']).unwrap_or((s, "0"));
    let num = |p: &str| p.trim().parse::<u64>().ok();
    let parts: Vec<&str> = hms.split(':').take(4).collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (num(h)?, num(m)?, num(s)?),
        [m, s] => (0, num(m)?, num(s)?),
        _ => return None,
    };
    if m >= 60 || sec >= 60 || h > 999 {
        return None;
    }
    // Thousandths: "5" is half a second.
    let frac: String = frac.trim().chars().take(3).collect();
    if frac.is_empty() || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let ms: u64 = format!("{frac:0<3}").parse().ok()?;
    Some(((h * 3600 + m * 60 + sec) * 1000 + ms) * 1000)
}

/// A subtitle's words without their markup: tags (`<i>`, `<v Ann>`, `<00:01.000>`,
/// `<font color=…>`) taken out, and `&amp;` and the like put back.
fn without_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            // A tag runs to its `>` (not far); a `<` that starts none stays.
            let ahead: Vec<char> = chars.clone().take(120).collect();
            let starts = ahead.first().is_some_and(|f| f.is_alphanumeric() || *f == '/' || *f == '.');
            if let Some(end) = ahead.iter().position(|a| *a == '>').filter(|_| starts) {
                for _ in 0..=end {
                    chars.next();
                }
                continue;
            }
        }
        out.push(c);
    }
    let mut s = out;
    for (from, to) in [
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&nbsp;", " "),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&lrm;", ""),
        ("&rlm;", ""),
        ("&amp;", "&"),
    ] {
        if s.contains(from) {
            s = s.replace(from, to);
        }
    }
    s
}

/// Who says a subtitle's words, by what it starts with: `You:` or `<v You>` are the owner's,
/// `Others:` or `<v Others>` the people heard through the computer; anything else has no
/// speaker named ([`Speaker::Media`]). The words without it.
fn said_by(words: &str) -> (Speaker, &str) {
    let w = words.trim_start();
    for (mark, s) in [("You:", Speaker::You), ("Others:", Speaker::Others), ("<v You>", Speaker::You), ("<v Others>", Speaker::Others)] {
        if let Some(rest) = w.strip_prefix(mark) {
            return (s, rest);
        }
    }
    (Speaker::Media, w)
}

/// Lines read from subtitles: SubRip (`.srt`) or WebVTT (`.vtt`; its heading, notes and styles
/// are passed over, a cue's settings after its times too). Who said a line is as [`said_by`]
/// reads it; markup goes. What isn't subtitles is left out; at most [`MAX_CUES`] lines.
pub fn from_subtitles(text: &str) -> Vec<Cue> {
    let mut out = Vec::new();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines().take(MAX_CUES * 8).peekable();
    while lines.peek().is_some() && out.len() < MAX_CUES {
        // A block: its number or name (optional), its times, its words up to an empty line.
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
        // Anything after the end time (a position, a cue's settings) is left out.
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (subtitle_time(a), subtitle_time(b)) else { continue };
        let raw = block.get(at + 1..).unwrap_or_default().join(" ");
        // Who said it: by its markup (`<v You>`), else by its words once the markup is gone.
        let (speaker, text) = match said_by(&raw) {
            (Speaker::Media, _) => {
                let plain = without_markup(&raw);
                let (speaker, words) = said_by(&plain);
                (speaker, clean_text(words))
            }
            (speaker, words) => (speaker, clean_text(&without_markup(words))),
        };
        if end > start && !text.is_empty() {
            out.push(Cue { start, end, speaker, text });
        }
    }
    out.sort_by_key(|c| (c.start, c.speaker));
    out
}

/// Lines timed on the edited video (subtitles brought in) put in the recording's time, so they
/// follow the edits as its own captions do. Each goes on its speaker's track (a line with no
/// speaker named on the microphone's, else the computer's sound's; a speaker with no track on
/// the other's), from where that track plays at its start to where it plays at its end. Lines
/// where none of the track's clips plays (a gap, a held frame, a clip played backwards) are left
/// out: the lines kept, and how many were left out.
pub fn from_timeline(lines: &[Cue], timeline: &Timeline) -> (Vec<Cue>, usize) {
    let track_of = |s: Speaker| -> Option<(Speaker, usize)> {
        let order = match s {
            Speaker::Others => [Speaker::Others, Speaker::You],
            Speaker::You | Speaker::Media => [Speaker::You, Speaker::Others],
        };
        order.into_iter().find_map(|sp| timeline.find(sp.source()).map(|i| (sp, i)))
    };
    let (mut out, mut left) = (Vec::new(), 0usize);
    for line in lines.iter().take(MAX_CUES) {
        let found = track_of(line.speaker).and_then(|(speaker, i)| {
            let clips: Vec<&crate::timeline::Clip> =
                timeline.tracks.get(i)?.clips.iter().filter(|c| !c.hold && !c.reverse && c.at < line.end && c.end() > line.start).collect();
            let (first, last) = (clips.first()?, clips.last()?);
            let start = first.source_at(line.start.max(first.at) - first.at);
            let end = last.source_at(line.end.min(last.end()) - last.at);
            // Clips moved out of the recording's order: the first clip's part only.
            let end = if end > start { end } else { first.source_at(line.end.min(first.end()) - first.at) };
            (end > start).then(|| Cue { start, end, speaker, text: line.text.clone() })
        });
        match found {
            Some(c) => out.push(c),
            None => left += 1,
        }
    }
    out.sort_by_key(|c| (c.start, c.speaker));
    (out, left)
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
    to_srt_named(&by_label(cues))
}

/// Each line with its speaker's own label (You, Others, none for a media file's).
fn by_label(cues: &[Cue]) -> Vec<(Cue, String)> {
    cues.iter().map(|c| (c.clone(), c.speaker.label().to_string())).collect()
}

/// What goes before a line's words: a name and a colon, or nothing.
fn said_by_name(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    match name.trim() {
        "" => String::new(),
        name => format!("{name}: "),
    }
}

/// The lines as SubRip subtitles, each with who said it as named (`Bill: …`; an empty name, none).
pub fn to_srt_named(lines: &[(Cue, String)]) -> String {
    let mut out = String::new();
    for (i, (c, who)) in lines.iter().enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n{}{}\n\n", i + 1, srt_time(c.start), srt_time(c.end), said_by_name(who), clean_text(&c.text)));
    }
    out
}

/// The lines as a plain transcript: a paragraph for each turn (the same speaker's lines less
/// than two seconds apart), with when it started.
pub fn to_text(cues: &[Cue]) -> String {
    to_text_named(&by_label(cues))
}

/// The lines as a plain transcript, each turn with who said it as named.
pub fn to_text_named(lines: &[(Cue, String)]) -> String {
    let mut out = String::new();
    let mut last: Option<(&str, u64)> = None;
    for (c, who) in lines {
        let text = clean_text(&c.text);
        match last {
            Some((s, end)) if s == who.as_str() && c.start <= end.saturating_add(2_000_000) => {
                out.push(' ');
                out.push_str(&text);
            }
            _ => {
                if last.is_some() {
                    out.push('\n');
                }
                out.push_str(&format!("[{}] {}{}", clock(c.start), said_by_name(who), text));
            }
        }
        last = Some((who.as_str(), last.filter(|(s, _)| *s == who.as_str()).map_or(c.end, |(_, e)| e.max(c.end))));
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Captions put right by hand: each works on the recording's lines (recording time, in order) and
/// keeps them in order, inside `0..duration`, and clean; a line that isn't there is an error.
pub mod edit {
    use super::{Cue, MAX_TEXT, clean_text};

    fn line(cues: &mut [Cue], i: usize) -> Result<&mut Cue, String> {
        cues.get_mut(i).ok_or_else(|| format!("there's no line {i} in the captions"))
    }

    fn sort(cues: &mut [Cue]) {
        cues.sort_by_key(|c| (c.start, c.speaker));
    }

    /// A line's words put right; no words left: the line goes.
    pub fn text(cues: &mut Vec<Cue>, i: usize, words: &str) -> Result<(), String> {
        let words = clean_text(words);
        if words.is_empty() {
            line(cues, i)?;
            cues.remove(i);
            return Ok(());
        }
        line(cues, i)?.text = words;
        Ok(())
    }

    /// A line cut in two between words, near its `at`th character (none: near its middle); its
    /// time shared as its words are.
    pub fn split(cues: &mut Vec<Cue>, i: usize, at: Option<usize>) -> Result<(), String> {
        let c = line(cues, i)?.clone();
        let chars: Vec<char> = c.text.chars().collect();
        let want = at.unwrap_or(chars.len() / 2).min(chars.len());
        // The space nearest the place asked for.
        let cut = chars
            .iter()
            .enumerate()
            .filter(|(_, ch)| ch.is_whitespace())
            .map(|(k, _)| k)
            .min_by_key(|k| k.abs_diff(want))
            .ok_or_else(|| "a line of one word can't be split".to_string())?;
        let first: String = chars.get(..cut).unwrap_or_default().iter().collect();
        let second: String = chars.get(cut..).unwrap_or_default().iter().collect();
        let (first, second) = (clean_text(&first), clean_text(&second));
        if first.is_empty() || second.is_empty() {
            return Err("there are no words on one side of that place".to_string());
        }
        let share = first.chars().count() as u64 + 1;
        let whole = share + second.chars().count() as u64;
        let mid = c.start + (c.end - c.start) * share / whole.max(1);
        let mid = mid.clamp(c.start.saturating_add(1).min(c.end), c.end);
        if let Some(l) = cues.get_mut(i) {
            l.text = first;
            l.end = mid;
        }
        cues.insert(i + 1, Cue { start: mid, end: c.end, speaker: c.speaker, text: second });
        sort(cues);
        Ok(())
    }

    /// A line and the next line of the same speaker made one.
    pub fn join(cues: &mut Vec<Cue>, i: usize) -> Result<(), String> {
        let c = line(cues, i)?.clone();
        let j = cues
            .iter()
            .enumerate()
            .skip(i + 1)
            .find(|(_, n)| n.speaker == c.speaker)
            .map(|(j, _)| j)
            .ok_or_else(|| "it's the speaker's last line: there's none after it to join".to_string())?;
        let next = cues.remove(j);
        let both = format!("{} {}", c.text, next.text);
        if both.chars().count() > MAX_TEXT {
            cues.insert(j, next);
            return Err(format!("together they'd be longer than a line can be ({MAX_TEXT} characters)"));
        }
        if let Some(l) = cues.get_mut(i) {
            l.text = clean_text(&both);
            l.end = l.end.max(next.end);
        }
        Ok(())
    }

    /// A line's start and end moved (either kept if not given), inside `0..duration`; it keeps
    /// at least a tenth of a second.
    pub fn times(cues: &mut [Cue], i: usize, start: Option<u64>, end: Option<u64>, duration: u64) -> Result<(), String> {
        let l = line(cues, i)?;
        let start = start.unwrap_or(l.start).min(duration);
        let end = end.unwrap_or(l.end).min(duration);
        if end < start.saturating_add(100_000) {
            return Err("a line must end at least a tenth of a second after it starts".to_string());
        }
        (l.start, l.end) = (start, end);
        sort(cues);
        Ok(())
    }
}

/// Words said while thinking, that mean nothing: what Take Out Filler Words cuts.
pub const FILLERS: &[&str] = &["um", "umm", "uh", "uhh", "uhm", "er", "erm", "err", "ah", "ahh", "hmm", "hm", "mm", "mmm"];

/// A word of a line, and when it's said (µs, in the line's own time: the recording's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    pub start: u64,
    pub end: u64,
    pub text: String,
}

/// `v × num / den`, without overflowing (`num` at most `den`: never more than `v`).
fn share(v: u64, num: u64, den: u64) -> u64 {
    u64::try_from(u128::from(v) * u128::from(num.min(den)) / u128::from(den.max(1))).unwrap_or(v)
}

/// A line's words, each with when it's said. The speech model gives only when each line starts
/// and ends, so these are estimated: each word is given a share of the line as long as it is
/// (its letters, and the space after it).
pub fn words(cue: &Cue) -> Vec<Word> {
    let list: Vec<&str> = cue.text.split_whitespace().take(MAX_TEXT).collect();
    let weight = |w: &str| w.chars().count() as u64 + 1;
    let total: u64 = list.iter().map(|w| weight(w)).sum();
    let len = cue.end.saturating_sub(cue.start);
    let mut so_far = 0u64;
    list.into_iter()
        .map(|w| {
            let start = cue.start.saturating_add(share(len, so_far, total));
            so_far += weight(w);
            Word { start, end: cue.start.saturating_add(share(len, so_far, total)), text: w.to_string() }
        })
        .collect()
}

/// A word as it's compared with the filler words: small letters, without the marks round it.
pub fn bare(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// The filler words (any of `list`, bare) in the lines heard on tracks that are in the video (not
/// muted): each line's place and the places of its filler words.
pub fn fillers(cues: &[Cue], timeline: &Timeline, list: &[String]) -> Vec<(usize, Vec<usize>)> {
    let heard = |s: Speaker| timeline.find(s.source()).and_then(|i| timeline.tracks.get(i)).is_some_and(|t| !t.muted);
    cues.iter()
        .enumerate()
        .filter(|(_, c)| heard(c.speaker))
        .filter_map(|(k, c)| {
            let found: Vec<usize> = words(c).iter().enumerate().filter(|(_, w)| list.contains(&bare(&w.text))).map(|(i, _)| i).collect();
            (!found.is_empty()).then_some((k, found))
        })
        .collect()
}

/// What cutting words out took: how many words, how much shorter the video is (µs), and where
/// the first stretch taken out was on the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordsCut {
    pub words: usize,
    pub cut: u64,
    pub at: u64,
}

/// Where a stretch of a speaker's recording (`from..to`) plays on the edited video: on each clip
/// of their track that plays it, unless the track is muted.
fn played_at(timeline: &Timeline, speaker: Speaker, from: u64, to: u64) -> Vec<(u64, u64)> {
    let Some(track) = timeline.find(speaker.source()).and_then(|i| timeline.tracks.get(i)) else { return Vec::new() };
    if track.muted {
        return Vec::new();
    }
    let mut out = Vec::new();
    for clip in track.clips.iter().filter(|c| !c.hold && !c.reverse) {
        let (a, b) = (from.max(clip.from), to.min(clip.from.saturating_add(clip.source_len())));
        if b <= a {
            continue;
        }
        let start = clip.at.saturating_add(clip.played(a - clip.from));
        let end = clip.at.saturating_add(clip.played(b - clip.from)).min(clip.end());
        if end > start {
            out.push((start, end));
        }
    }
    out
}

/// Words of the recording's lines cut out of the video: where each is said (on its speaker's
/// track) is taken out of every track, closing the gap, and the words out of their line (a line
/// with words cut from its middle becomes two). `picks` are lines' places in `cues` (the
/// recording's lines, in order) with the places of their words (as [`words`] has them); words no
/// longer in the video (cut already, or muted) are left as they are. Nothing in the video: an
/// error, and nothing changes.
pub fn cut_words(timeline: &mut Timeline, cues: &mut Vec<Cue>, picks: &[(usize, Vec<usize>)]) -> Result<WordsCut, String> {
    use std::collections::{BTreeMap, BTreeSet};
    // Each line once, its words in order.
    let mut by_line: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (line, chosen) in picks.iter().take(MAX_CUES) {
        let cue = cues.get(*line).ok_or_else(|| format!("there's no line {line} in the captions"))?;
        let n = words(cue).len();
        let set = by_line.entry(*line).or_default();
        for w in chosen.iter().take(MAX_TEXT) {
            if *w >= n {
                return Err(format!("line {line} has no word {w} (it has {n})"));
            }
            set.insert(*w);
        }
    }
    by_line.retain(|_, set| !set.is_empty());
    if by_line.is_empty() {
        return Err("no words were chosen".to_string());
    }
    // Where the words of each run are said in the video; the words found there are cut.
    let mut stretches: Vec<(u64, u64)> = Vec::new();
    let mut hit: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (line, set) in &by_line {
        let Some(cue) = cues.get(*line) else { continue };
        let ws = words(cue);
        let list: Vec<usize> = set.iter().copied().collect();
        for run in list.chunk_by(|a, b| a + 1 == *b) {
            let (Some(first), Some(last)) = (run.first().and_then(|k| ws.get(*k)), run.last().and_then(|k| ws.get(*k))) else { continue };
            let found = played_at(timeline, cue.speaker, first.start, last.end);
            if !found.is_empty() {
                hit.entry(*line).or_default().extend(run.iter().copied());
                stretches.extend(found);
            }
        }
    }
    // Overlapping or touching stretches taken out as one.
    stretches.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (a, b) in stretches {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    let Some(&(at, _)) = merged.first() else {
        return Err("those words aren't in the video as edited: they were cut, or their sound is muted".to_string());
    };
    // The last first, so the places of the others hold.
    let mut cut = 0u64;
    for (a, b) in merged.iter().rev() {
        timeline.cut(*a, *b).map_err(|e| e.to_string())?;
        cut = cut.saturating_add(b - a);
    }
    // The words out of their lines: what's left of each in pieces, each said when its words are.
    let old = std::mem::take(cues);
    for (k, cue) in old.into_iter().enumerate() {
        let Some(gone) = hit.get(&k) else {
            cues.push(cue);
            continue;
        };
        let mut piece: Vec<Word> = Vec::new();
        let flush = |piece: &mut Vec<Word>, cues: &mut Vec<Cue>| {
            if let (Some(first), Some(last)) = (piece.first(), piece.last()) {
                let text = clean_text(&piece.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "));
                if last.end > first.start && !text.is_empty() {
                    cues.push(Cue { start: first.start, end: last.end, speaker: cue.speaker, text });
                }
            }
            piece.clear();
        };
        for (i, w) in words(&cue).into_iter().enumerate() {
            if gone.contains(&i) {
                flush(&mut piece, cues);
            } else {
                piece.push(w);
            }
        }
        flush(&mut piece, cues);
    }
    cues.sort_by_key(|c| (c.start, c.speaker));
    Ok(WordsCut { words: hit.values().map(BTreeSet::len).sum(), cut, at })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{CameraLayout, Clip, Track};

    fn cue(start: u64, end: u64, speaker: Speaker, text: &str) -> Cue {
        Cue { start, end, speaker, text: text.to_string() }
    }

    fn timeline() -> Timeline {
        let track = |stream, kind| Track {
            stream,
            kind,
            muted: false,
            volume: 1.0,
            clips: vec![Clip::new(0, 0, 20_000_000)],
            name: None,
            denoise: 0.0,
            points: Vec::new(),
            locked: false,
            solo: false,
            mono: false,
        };
        Timeline {
            tracks: vec![track(0, StreamKind::Screen), track(1, StreamKind::SystemSound), track(2, StreamKind::Microphone)],
            camera: CameraLayout::default(),
            captions: Default::default(),
            layers: Vec::new(),
            media: Vec::new(),
            pointer: Default::default(),
            corrected: None,
            markers: Vec::new(),
            keys: Default::default(),
            ducking: Default::default(),
            watermark: Default::default(),
            frame: Default::default(),
            motion_blur: Default::default(),
            pointer_hidden: Vec::new(),
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
    fn lines_are_put_right_split_joined_and_moved() {
        let mut cues = vec![
            cue(1_000_000, 3_000_000, Speaker::You, "Hello their, this is the new page."),
            cue(2_000_000, 4_000_000, Speaker::Others, "Hi."),
            cue(5_000_000, 6_000_000, Speaker::You, "Any questions?"),
        ];
        edit::text(&mut cues, 0, "Hello there,\n this is   the new page.").unwrap();
        assert_eq!(cues[0].text, "Hello there, this is the new page.");
        // Split near its middle, between words; the time shared as the words are.
        edit::split(&mut cues, 0, None).unwrap();
        assert_eq!((cues[0].text.as_str(), cues[2].text.as_str()), ("Hello there, this", "is the new page."));
        assert!(cues[0].end > 1_900_000 && cues[0].end < 2_200_000 && cues[2].start == cues[0].end && cues[2].end == 3_000_000, "{cues:?}");
        assert_eq!(cues[1].speaker, Speaker::Others, "kept in order: the others' line starts before the second half");
        // At a place: the space nearest it.
        edit::split(&mut cues, 0, Some(3)).unwrap();
        assert_eq!(cues[0].text, "Hello");
        // Joined with the speaker's next line (the others' line between is passed over).
        edit::join(&mut cues, 0).unwrap();
        assert_eq!(cues[0].text, "Hello there, this");
        let mine = cues.iter().position(|c| c.text == "is the new page.").unwrap();
        edit::join(&mut cues, mine).unwrap();
        assert_eq!(cues[mine].text, "is the new page. Any questions?");
        assert_eq!(cues[mine].end, 6_000_000);
        assert!(edit::join(&mut cues, mine).is_err(), "the speaker's last line");
        // Moved: kept inside the recording, and in order.
        edit::times(&mut cues, 0, Some(4_500_000), Some(99_000_000), 7_000_000).unwrap();
        assert_eq!(cues.last().map(|c| (c.start, c.end)), Some((4_500_000, 7_000_000)));
        assert!(edit::times(&mut cues, 0, Some(10), Some(20), 7_000_000).is_err(), "too short");
        // No words left: the line goes. A line that isn't there, one word: errors.
        let n = cues.len();
        edit::text(&mut cues, 0, "  \n").unwrap();
        assert_eq!(cues.len(), n - 1);
        assert!(edit::text(&mut cues, 9, "x").is_err() && edit::split(&mut cues, 9, None).is_err());
        let mut one = vec![cue(0, 1_000_000, Speaker::You, "Word")];
        assert!(edit::split(&mut one, 0, None).is_err());
    }

    #[test]
    fn each_piece_on_the_timeline_says_which_line_it_is_and_corrections_replace_what_was_heard() {
        let caps = Captions { cues: vec![cue(1_000_000, 5_000_000, Speaker::You, "A long line.")], model: "m".into() };
        let mut tl = timeline();
        tl.cut(2_000_000, 3_000_000).unwrap();
        let on = caps.on_timeline_from(&tl);
        assert_eq!(on.iter().map(|(_, k)| *k).collect::<Vec<_>>(), vec![0, 0], "both pieces are line 0");
        assert!(as_corrected(Some(&caps), &tl).is_some_and(|c| c.cues[0].text == "A long line."));
        tl.corrected = Some(vec![cue(1_000_000, 5_000_000, Speaker::You, "A longer line.")]);
        let now = as_corrected(Some(&caps), &tl).unwrap();
        assert_eq!((now.cues[0].text.as_str(), now.model.as_str()), ("A longer line.", "m"));
        let lines = transcript_from(Some(&now), &tl, &Default::default());
        assert!(lines.iter().all(|(c, _, k)| c.text == "A longer line." && *k == Some(0)));
    }

    #[test]
    fn a_lines_words_are_given_times_by_their_length() {
        let ws = words(&cue(1_000_000, 2_000_000, Speaker::You, "So um this"));
        // "So " 3, "um " 3, "this " 5 of 11.
        let times: Vec<(u64, u64)> = ws.iter().map(|w| (w.start, w.end)).collect();
        assert_eq!(times, vec![(1_000_000, 1_272_727), (1_272_727, 1_545_454), (1_545_454, 2_000_000)]);
        assert_eq!(ws[1].text, "um");
        assert!(words(&cue(5, 5, Speaker::You, "")).is_empty());
        assert_eq!(bare("Um,"), "um");
        assert_eq!(bare("“UH…”"), "uh");
        assert_eq!(bare("uh-huh"), "uh-huh", "only the marks round it go");
    }

    #[test]
    fn words_are_cut_out_of_the_video_and_their_line() {
        let mut tl = timeline();
        let mut cues = vec![
            cue(1_000_000, 2_000_000, Speaker::You, "So um this"),
            cue(1_500_000, 3_000_000, Speaker::Others, "Uh, fine."),
            cue(4_000_000, 5_000_000, Speaker::You, "Hello there"),
        ];
        // "um": 1.272727 s to 1.545454 s, out of every track.
        let done = cut_words(&mut tl, &mut cues, &[(0, vec![1])]).unwrap();
        assert_eq!(done, WordsCut { words: 1, cut: 272_727, at: 1_272_727 });
        assert_eq!(tl.length(), 20_000_000 - 272_727);
        assert_eq!(tl.tracks[0].clips.len(), 2, "the screen too");
        let texts: Vec<&str> = cues.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, ["So", "Uh, fine.", "this", "Hello there"], "in order of their start");
        assert_eq!((cues[0].start, cues[0].end, cues[2].start, cues[2].end), (1_000_000, 1_272_727, 1_545_454, 2_000_000));
        // On the timeline the two halves meet where the word was.
        let on = Captions { cues: cues.clone(), model: String::new() }.on_timeline(&tl);
        let mine: Vec<(u64, u64)> = on.iter().filter(|c| c.speaker == Speaker::You).map(|c| (c.start, c.end)).take(2).collect();
        assert_eq!(mine, vec![(1_000_000, 1_272_727), (1_272_727, 1_727_273)]);
        // The filler words of every line heard: the others' "Uh," (their first word), none of mine left.
        let list: Vec<String> = FILLERS.iter().map(|s| s.to_string()).collect();
        assert_eq!(fillers(&cues, &tl, &list), vec![(1, vec![0])]);
        // Muted, a track's words aren't in the video: none to find or cut.
        tl.set_muted(1, true).unwrap();
        assert!(fillers(&cues, &tl, &list).is_empty());
        let before = (tl.clone(), cues.clone());
        assert!(cut_words(&mut tl, &mut cues, &[(1, vec![0])]).is_err());
        assert_eq!((tl.clone(), cues.clone()), before);
        // Two words side by side are one stretch; the line's end goes.
        let line = cues.iter().position(|c| c.text == "Hello there").unwrap();
        let done = cut_words(&mut tl, &mut cues, &[(line, vec![1, 0])]).unwrap();
        assert_eq!((done.words, done.cut), (2, 1_000_000));
        assert!(!cues.iter().any(|c| c.text.contains("Hello")));
        // Junk: lines and words that aren't there are errors.
        for bad in [vec![(99, vec![0])], vec![(0, vec![9])]] {
            assert!(cut_words(&mut tl, &mut cues, &bad).is_err());
        }
        assert!(cut_words(&mut tl, &mut cues, &[]).is_err(), "nothing chosen");
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
    fn webvtt_subtitles_are_read_with_their_markup_taken_out() {
        let vtt = "WEBVTT - a talk\n\nNOTE written by hand --> not a cue\n\nSTYLE\n::cue { color: lime }\n\n\
                   intro\n00:01.500 --> 00:03.000 align:start line:90%\n<v You>Hello <b>there</b> &amp; welcome\n\n\
                   01:02:03.5 --> 01:02:04.000\n<v Ann>Hi <00:00:03.600>Bo &lt;3\n\n\
                   00:05.000 --> 00:06.000\n<i>Others:</i> fine\n";
        let cues = from_subtitles(vtt);
        assert_eq!(
            cues,
            vec![
                cue(1_500_000, 3_000_000, Speaker::You, "Hello there & welcome"),
                cue(5_000_000, 6_000_000, Speaker::Others, "fine"),
                cue(3_723_500_000, 3_724_000_000, Speaker::Media, "Hi Bo <3"),
            ]
        );
        // SubRip's markup goes too, and a "<" that starts no tag stays.
        assert_eq!(from_srt("1\n00:00:01,000 --> 00:00:02,000\n<font color=\"#fff\">a < b</font>\n")[0].text, "a < b");
        // Junk: never panicked over.
        for junk in ["WEBVTT", "00:61.000 --> 00:62.000\nx", "<<<<-->>>>", "1:2:3:4.5 --> 9\nx", "00:01.x --> 00:02.000\ny", "\u{feff}-->\n<"] {
            let _ = from_subtitles(junk);
        }
        assert!(from_subtitles("00:01.x --> 00:02.000\ny").is_empty());
        assert!(from_subtitles(&format!("00:01.000 --> 00:02.000\n<{}\n", "ab ".repeat(5_000))).len() == 1);
    }

    #[test]
    fn subtitles_timed_on_the_edited_video_follow_its_tracks_from_then_on() {
        let mut tl = timeline();
        // Two seconds taken out at 1 s: the video's 2 s is the recording's 4 s.
        tl.cut(1_000_000, 3_000_000).unwrap();
        let lines = vec![
            cue(2_000_000, 2_500_000, Speaker::Media, "Mine, as no one is named"),
            cue(500_000, 1_500_000, Speaker::Others, "Across the cut"),
            cue(30_000_000, 31_000_000, Speaker::You, "After the end"),
        ];
        let (cues, left) = from_timeline(&lines, &tl);
        assert_eq!(left, 1);
        assert_eq!(
            cues,
            vec![cue(500_000, 3_500_000, Speaker::Others, "Across the cut"), cue(4_000_000, 4_500_000, Speaker::You, "Mine, as no one is named")]
        );
        // As the recording's captions, they fall where they were brought in.
        let on = Captions { cues, model: String::new() }.on_timeline(&tl);
        assert!(on.iter().any(|c| c.text.starts_with("Mine") && c.start == 2_000_000 && c.end == 2_500_000), "{on:?}");
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

    #[test]
    fn the_transcript_says_who_said_each_line_by_the_names_given() {
        use crate::layers::{Item, MediaItem, MediaKind, What};
        let mut tl = timeline();
        let song = MediaItem {
            name: "Song.mp3".into(),
            path: "C:/Music/Song.mp3".into(),
            kind: MediaKind::Sound,
            duration: 30_000_000,
            ..MediaItem::default()
        };
        let id = tl.add_media(song).unwrap();
        let item = Item { len: 4_000_000, from: 0, what: What::Media { id }, ..Item::default() };
        let (layer, _) = tl.place(item, None, 2_000_000).unwrap();
        let mut of = std::collections::HashMap::new();
        of.insert(id, vec![cue(1_000_000, 2_000_000, Speaker::Media, "From the song.")]);
        let caps = Captions {
            cues: vec![cue(1_000_000, 2_000_000, Speaker::You, "Hello."), cue(5_000_000, 6_000_000, Speaker::Others, "Hi back.")],
            model: String::new(),
        };
        let lines = transcript(Some(&caps), &tl, &of);
        let said: Vec<(&str, Who)> = lines.iter().map(|(c, w)| (c.text.as_str(), *w)).collect();
        assert_eq!(
            said,
            vec![("Hello.", Who::Speaker(Speaker::You)), ("From the song.", Who::Layer(layer)), ("Hi back.", Who::Speaker(Speaker::Others))]
        );
        assert_eq!(lines[1].0.start, 3_000_000, "on the timeline, where the song is");
        // Named: You and Others, and the layer by its name.
        assert_eq!(speaker_name(&tl, Who::Speaker(Speaker::You)), "You");
        assert_eq!(speaker_name(&tl, Who::Speaker(Speaker::Others)), "Others");
        assert_eq!(speaker_name(&tl, Who::Layer(layer)), "Sound 1");
        // Renamed: the microphone's track, the computer's sound's, the layer.
        let mic = tl.find(StreamKind::Microphone).unwrap();
        tl.rename_track(mic, "Bill").unwrap();
        let sound = tl.find(StreamKind::SystemSound).unwrap();
        tl.rename_track(sound, "Guest").unwrap();
        tl.set_layer(layer, None, None, Some("Theme song")).unwrap();
        assert_eq!(speaker_name(&tl, Who::Speaker(Speaker::You)), "Bill");
        assert_eq!(speaker_name(&tl, Who::Speaker(Speaker::Others)), "Guest");
        assert_eq!(speaker_name(&tl, Who::Layer(layer)), "Theme song");
        assert_eq!(speaker_name(&tl, Who::Layer(999)), "Media", "a layer gone");
        // Written out with those names.
        let named: Vec<(Cue, String)> = lines.iter().map(|(c, w)| (c.clone(), speaker_name(&tl, *w))).collect();
        let text = to_text_named(&named);
        assert!(text.contains("] Bill: Hello.") && text.contains("] Theme song: From the song.") && text.contains("] Guest: Hi back."), "{text}");
        assert!(to_srt_named(&named).contains("\nBill: Hello.\n"));
    }
}
