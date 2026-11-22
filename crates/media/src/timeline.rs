//! The edits to a recording: each track's clips on the timeline, its volume, and where the
//! camera sits on the picture. The recording itself never changes; a clip says which stretch
//! of its track plays when.
//!
//! Times are in microseconds. Every edit keeps the clips of a track in order, apart, and inside
//! what was recorded; anything else is refused with an error.

use serde::{Deserialize, Serialize};

use crate::recording::{Recording, StreamKind};

/// The shortest clip an edit leaves (shorter pieces are dropped).
pub const MIN_CLIP_US: u64 = 20_000;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EditError {
    #[error("there is no track {0}")]
    NoTrack(usize),
    #[error("there is no clip {1} on track {0}")]
    NoClip(usize, usize),
    #[error("{0}")]
    Invalid(String),
}

/// How fast a clip plays, in hundredths of as recorded: 100 as recorded, 200 twice as fast.
pub const NORMAL_SPEED: u16 = 100;
/// The slowest a clip plays (a tenth of as recorded).
pub const MIN_SPEED: u16 = 10;
/// The fastest a clip plays (ten times as recorded).
pub const MAX_SPEED: u16 = 1000;

fn normal_speed() -> u16 {
    NORMAL_SPEED
}

fn is_normal_speed(s: &u16) -> bool {
    *s == NORMAL_SPEED
}

/// `v × num / den`, without overflowing.
fn scale(v: u64, num: u64, den: u64) -> u64 {
    u64::try_from(u128::from(v) * u128::from(num) / u128::from(den.max(1))).unwrap_or(u64::MAX)
}

/// A stretch of a track's recording, placed on the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clip {
    /// Where it starts on the timeline.
    pub at: u64,
    /// Where it starts in the recording.
    pub from: u64,
    /// How long it plays on the timeline.
    pub len: u64,
    /// How fast it plays, in hundredths ([`NORMAL_SPEED`] as recorded); edits saved before clips
    /// had speeds play as recorded.
    #[serde(default = "normal_speed", skip_serializing_if = "is_normal_speed")]
    pub speed: u16,
    /// How long its sound takes to come up from silence at its start (µs of the timeline); 0:
    /// at once. Sound only.
    #[serde(default, rename = "fadeIn", skip_serializing_if = "is_zero")]
    pub fade_in: u64,
    /// How long its sound takes to go down to silence at its end.
    #[serde(default, rename = "fadeOut", skip_serializing_if = "is_zero")]
    pub fade_out: u64,
    /// A held frame: the picture at `from` shown for all its length (a picture track only).
    #[serde(default, skip_serializing_if = "is_false")]
    pub hold: bool,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// The longest fade.
pub const MAX_FADE_US: u64 = 30_000_000;
/// The longest a frame is held.
pub const MAX_HOLD_US: u64 = 600_000_000;

impl Default for Clip {
    fn default() -> Clip {
        Clip::new(0, 0, 0)
    }
}

impl Clip {
    /// A clip playing as recorded.
    pub const fn new(at: u64, from: u64, len: u64) -> Clip {
        Clip { at, from, len, speed: NORMAL_SPEED, fade_in: 0, fade_out: 0, hold: false }
    }

    /// A held frame: the picture recorded at `from`, shown from `at` for `len`.
    pub const fn held(at: u64, from: u64, len: u64) -> Clip {
        Clip { hold: true, ..Clip::new(at, from, len) }
    }

    pub fn end(&self) -> u64 {
        self.at.saturating_add(self.len)
    }

    /// Its first `len` µs: the rest cut off (and its fade out with it).
    fn head(&self, len: u64) -> Clip {
        if len >= self.len {
            return *self;
        }
        Clip { len, fade_out: 0, ..*self }
    }

    /// What follows its first `skip` µs, where it was (its fade in left behind).
    fn tail(&self, skip: u64) -> Clip {
        if skip == 0 {
            return *self;
        }
        let skip = skip.min(self.len);
        Clip { at: self.at.saturating_add(skip), from: self.source_at(skip), len: self.len - skip, fade_in: 0, ..*self }
    }

    /// How loud its sound is `d` µs into it, faded in and out: 0 to 1. Fades longer than the
    /// clip share it.
    pub fn fade_at(&self, d: u64) -> f32 {
        let (fi, fo) = (self.fade_in.min(MAX_FADE_US), self.fade_out.min(MAX_FADE_US));
        if fi == 0 && fo == 0 {
            return 1.0;
        }
        let both = fi.saturating_add(fo);
        let (fi, fo) = if both > self.len { (scale(fi, self.len, both), scale(fo, self.len, both)) } else { (fi, fo) };
        // A smooth rise (a quarter of a sine wave, squared).
        let rise = |x: f32| (x.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2).sin().powi(2);
        let mut g = 1.0;
        if fi > 0 && d < fi {
            g *= rise(d as f32 / fi as f32);
        }
        let left = self.len.saturating_sub(d);
        if fo > 0 && left < fo {
            g *= rise(left as f32 / fo as f32);
        }
        g
    }

    /// Its speed in hundredths, kept in range.
    fn pace(&self) -> u64 {
        u64::from(self.speed.clamp(MIN_SPEED, MAX_SPEED))
    }

    /// Its speed: 1 as recorded, 2 twice as fast.
    pub fn rate(&self) -> f64 {
        self.pace() as f64 / 100.0
    }

    /// Where in the recording it is `d` µs after its start on the timeline (a held frame: where
    /// it was held, all along).
    pub fn source_at(&self, d: u64) -> u64 {
        if self.hold {
            return self.from;
        }
        self.from.saturating_add(scale(d, self.pace(), 100))
    }

    /// How much of the recording it plays (µs): none for a held frame.
    pub fn source_len(&self) -> u64 {
        if self.hold {
            return 0;
        }
        scale(self.len, self.pace(), 100)
    }

    /// How long `src` µs of the recording take to play at its speed.
    pub fn played(&self, src: u64) -> u64 {
        scale(src, 100, self.pace())
    }
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub stream: u8,
    pub kind: StreamKind,
    #[serde(default)]
    pub muted: bool,
    /// 0 to 2 (1 is as recorded). Sound only.
    #[serde(default = "one")]
    pub volume: f32,
    pub clips: Vec<Clip>,
    /// The name the owner gave it (its header, and the transcript's speaker for the words heard
    /// on it); none: what it is (Microphone), and You or Others in the transcript.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// How much of its background noise (a fan, a hum, hiss) is taken out: 0 none, to 1 the most.
    /// Sound only.
    #[serde(default, skip_serializing_if = "is_quiet")]
    pub denoise: f32,
}

fn is_quiet(v: &f32) -> bool {
    !v.is_finite() || *v <= 0.0
}

/// The longest name a track or a layer is given.
pub const MAX_NAME: usize = 60;

/// A name as it's kept: one line, no control characters, not too long; none if it's empty.
pub fn clean_name(name: &str) -> Option<String> {
    let s: String = name.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect();
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CameraShape {
    Rectangle,
    #[default]
    Rounded,
    Circle,
}

/// The camera's picture on top of the screen's.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraLayout {
    #[serde(default)]
    pub corner: Corner,
    /// Its width as a share of the video's, 0.1 to 0.5.
    #[serde(default = "camera_size")]
    pub size: f32,
    #[serde(default)]
    pub shape: CameraShape,
    /// Dragged on the picture: its middle (shares of the video's width and height), else it's
    /// in its corner.
    #[serde(default)]
    pub at: Option<[f32; 2]>,
}

fn camera_size() -> f32 {
    0.22
}

impl Default for CameraLayout {
    fn default() -> CameraLayout {
        CameraLayout { corner: Corner::default(), size: camera_size(), shape: CameraShape::default(), at: None }
    }
}

/// Whether the captions show: on the picture in the editor, and burned into the video made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionOptions {
    #[serde(default = "yes")]
    pub show: bool,
    #[serde(default)]
    pub burn: bool,
}

fn yes() -> bool {
    true
}

impl Default for CaptionOptions {
    fn default() -> CaptionOptions {
        CaptionOptions { show: true, burn: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub camera: CameraLayout,
    #[serde(default)]
    pub captions: CaptionOptions,
    /// Layers of items added over and beside the recording (see [`crate::layers`]), the later
    /// ones on top.
    #[serde(default)]
    pub layers: Vec<crate::layers::Layer>,
    /// The media bin: files brought in for the layers.
    #[serde(default)]
    pub media: Vec<crate::layers::MediaItem>,
    /// How the mouse pointer is drawn on the video (if the recording has it).
    #[serde(default)]
    pub pointer: crate::pointer::Look,
    /// The recording's captions as put right by hand (words, lines split or joined, times
    /// moved), in recording time; none: as they were heard. See [`crate::captions::edit`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corrected: Option<Vec<crate::captions::Cue>>,
    /// Moments marked on the timeline, in order: the chapters of the video made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<Marker>,
}

/// A moment marked on the timeline, with a name: a chapter of the video made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub at: u64,
    pub name: String,
}

/// The most markers a video has (as many as an MP4's chapter list holds).
pub const MAX_MARKERS: usize = 255;

/// A marker's name as it's kept: one clean line, else "Chapter n".
fn marker_name(name: &str, n: usize) -> String {
    clean_name(name).unwrap_or_else(|| format!("Chapter {}", n.max(1)))
}

impl Timeline {
    /// Every track whole, as recorded.
    pub fn new(rec: &Recording) -> Timeline {
        let tracks = rec
            .header
            .streams
            .iter()
            .map(|s| {
                let len = rec.stream_duration(s.id);
                let clips = if len >= MIN_CLIP_US { vec![Clip::new(0, 0, len)] } else { Vec::new() };
                Track { stream: s.id, kind: s.kind, muted: false, volume: 1.0, clips, name: None, denoise: 0.0 }
            })
            .collect();
        Timeline {
            tracks,
            camera: CameraLayout::default(),
            captions: CaptionOptions::default(),
            layers: Vec::new(),
            media: Vec::new(),
            pointer: Default::default(),
            corrected: None,
            markers: Vec::new(),
        }
    }

    /// Edits read from a file, made safe for this recording: tracks it doesn't have go, tracks
    /// it has come back whole, clips are kept inside what was recorded and apart.
    pub fn checked(mut self, rec: &Recording) -> Timeline {
        let fresh = Timeline::new(rec);
        // In the order they were left in (the owner may have moved them), each once; then any the
        // recording has that weren't saved.
        let mut order: Vec<Track> = Vec::new();
        for t in std::mem::take(&mut self.tracks).into_iter().take(64) {
            if fresh.tracks.iter().any(|f| f.stream == t.stream) && !order.iter().any(|o| o.stream == t.stream) {
                order.push(t);
            }
        }
        for f in &fresh.tracks {
            if !order.iter().any(|o| o.stream == f.stream) {
                order.push(f.clone());
            }
        }
        let mut tracks = Vec::new();
        for mut t in order {
            let Some(f) = fresh.tracks.iter().find(|f| f.stream == t.stream) else { continue };
            t.kind = f.kind;
            t.name = t.name.as_deref().and_then(clean_name);
            t.volume = if t.volume.is_finite() { t.volume.clamp(0.0, 2.0) } else { 1.0 };
            t.denoise = if t.denoise.is_finite() && !t.kind.is_video() { t.denoise.clamp(0.0, 1.0) } else { 0.0 };
            let sound = !t.kind.is_video();
            let source = rec.stream_duration(t.stream);
            let mut clips: Vec<Clip> = t
                .clips
                .iter()
                .filter_map(|c| {
                    // A held frame: a picture track's, of a moment recorded, not too long.
                    if c.hold {
                        let len = c.len.min(MAX_HOLD_US);
                        return (!sound && source > 0 && len >= MIN_CLIP_US).then_some(Clip::held(
                            c.at.min(u64::MAX / 4),
                            c.from.min(source - 1),
                            len,
                        ));
                    }
                    let speed = c.speed.clamp(MIN_SPEED, MAX_SPEED);
                    let from = c.from.min(source);
                    let most = Clip { speed, ..Clip::new(0, from, 0) }.played(source - from);
                    let len = c.len.min(most);
                    let fade = |f: u64| if sound { f.min(MAX_FADE_US) } else { 0 };
                    (len >= MIN_CLIP_US).then_some(Clip {
                        at: c.at.min(u64::MAX / 4),
                        from,
                        len,
                        speed,
                        fade_in: fade(c.fade_in),
                        fade_out: fade(c.fade_out),
                        hold: false,
                    })
                })
                .take(10_000)
                .collect();
            clips.sort_by_key(|c| c.at);
            let mut kept: Vec<Clip> = Vec::with_capacity(clips.len());
            for c in clips {
                if kept.last().is_none_or(|l| l.end() <= c.at) {
                    kept.push(c);
                }
            }
            t.clips = kept;
            tracks.push(t);
        }
        self.camera.size = if self.camera.size.is_finite() { self.camera.size.clamp(0.1, 0.5) } else { camera_size() };
        self.camera.at = self.camera.at.filter(|[x, y]| x.is_finite() && y.is_finite()).map(|[x, y]| [x.clamp(-0.5, 1.5), y.clamp(-0.5, 1.5)]);
        let mut checked = Timeline {
            tracks,
            camera: self.camera,
            captions: self.captions,
            layers: self.layers,
            media: self.media,
            pointer: self.pointer.checked(),
            corrected: self.corrected.map(|c| crate::captions::Captions { cues: c, model: String::new() }.checked(rec.duration_us).cues),
            markers: Vec::new(),
        };
        checked.check_layers();
        // The markers: named, in order, one to a moment, not too many.
        let mut markers: Vec<Marker> =
            self.markers.into_iter().take(MAX_MARKERS * 4).map(|m| Marker { at: m.at.min(u64::MAX / 4), name: marker_name(&m.name, 0) }).collect();
        markers.sort_by_key(|m| m.at);
        markers.dedup_by_key(|m| m.at);
        markers.truncate(MAX_MARKERS);
        checked.markers = markers;
        checked
    }

    /// How long the edited video is: to the end of the last clip, or of the last item added.
    pub fn length(&self) -> u64 {
        self.tracks.iter().filter_map(|t| t.clips.last()).map(Clip::end).max().unwrap_or(0).max(self.layers_length())
    }

    pub fn track(&self, i: usize) -> Result<&Track, EditError> {
        self.tracks.get(i).ok_or(EditError::NoTrack(i))
    }

    fn track_mut(&mut self, i: usize) -> Result<&mut Track, EditError> {
        self.tracks.get_mut(i).ok_or(EditError::NoTrack(i))
    }

    /// The first track of a kind.
    pub fn find(&self, kind: StreamKind) -> Option<usize> {
        self.tracks.iter().position(|t| t.kind == kind)
    }

    /// Where in its recording track `i` is at timeline time `t`, if a clip is playing then.
    pub fn source_time(&self, i: usize, t: u64) -> Option<u64> {
        let track = self.tracks.get(i)?;
        let c = track.clips.iter().find(|c| c.at <= t && t < c.end())?;
        Some(c.source_at(t - c.at))
    }

    /// The clip of track `i` playing at `t`.
    pub fn clip_at(&self, i: usize, t: u64) -> Option<usize> {
        self.tracks.get(i)?.clips.iter().position(|c| c.at <= t && t < c.end())
    }

    /// Takes `from..to` out of every track and closes the gap.
    pub fn cut(&mut self, from: u64, to: u64) -> Result<(), EditError> {
        if to <= from {
            return Err(EditError::Invalid("the stretch to cut is empty".to_string()));
        }
        let gap = to - from;
        for t in &mut self.tracks {
            let mut out = Vec::with_capacity(t.clips.len() + 1);
            for c in &t.clips {
                // The part before the cut, and the part after it moved back.
                if c.at < from {
                    out.push(c.head(from - c.at));
                }
                if c.end() > to {
                    let mut after = c.tail(to.saturating_sub(c.at));
                    after.at = c.at.max(to) - gap;
                    out.push(after);
                }
            }
            out.retain(|c| c.len >= MIN_CLIP_US);
            t.clips = out;
        }
        self.cut_layers(from, to);
        // Markers in the stretch go to where it was; those after move up.
        for m in &mut self.markers {
            m.at = if m.at >= to { m.at - gap } else { m.at.min(from) };
        }
        self.markers.dedup_by_key(|m| m.at);
        Ok(())
    }

    /// A copy of the edits with only `from..to` left, starting at 0: what a video of just that
    /// stretch is made from. The markers in it stay (moved up with it); those outside it go.
    pub fn stretch(&self, from: u64, to: u64) -> Result<Timeline, EditError> {
        let to = to.min(self.length());
        if to <= from {
            return Err(EditError::Invalid("the stretch is empty".to_string()));
        }
        let mut t = self.clone();
        t.markers.retain(|m| (from..to).contains(&m.at));
        let end = t.length();
        if end > to {
            t.cut(to, end)?;
        }
        if from > 0 {
            t.cut(0, from)?;
        }
        Ok(t)
    }

    /// Pushes everything from `at` on along by `len` on every track and layer, leaving a gap
    /// there: a clip playing at `at` is split and its second half moved. Items over `at` that
    /// show something (titles, overlays, pictures) stay on through the gap; a sound's or a video's
    /// goes on after it. Markers after `at` move along.
    fn insert_time(&mut self, at: u64, len: u64) {
        for t in &mut self.tracks {
            let mut out = Vec::with_capacity(t.clips.len() + 1);
            for c in &t.clips {
                if c.end() <= at {
                    out.push(*c);
                } else if c.at >= at {
                    out.push(Clip { at: c.at.saturating_add(len), ..*c });
                } else {
                    out.push(c.head(at - c.at));
                    let mut after = c.tail(at - c.at);
                    after.at = at.saturating_add(len);
                    out.push(after);
                }
            }
            out.retain(|c| c.len >= MIN_CLIP_US);
            t.clips = out;
        }
        self.insert_time_layers(at, len);
        for m in &mut self.markers {
            if m.at > at {
                m.at = m.at.saturating_add(len);
            }
        }
    }

    /// Holds the picture at `at` for `len` (a freeze frame): every track waits meanwhile and
    /// what follows moves along by `len`, the sound silent while it's held. Each picture track
    /// holds the moment it showed at `at` (or, at the end of a clip, its last); one with no
    /// picture there shows none. Returns the stretch held.
    pub fn hold_frame(&mut self, at: u64, len: u64) -> Result<(u64, u64), EditError> {
        if len < MIN_CLIP_US {
            return Err(EditError::Invalid("a frame is held for a moment at least".to_string()));
        }
        let (at, len) = (at.min(u64::MAX / 4), len.min(MAX_HOLD_US));
        let held: Vec<(usize, u64)> = self
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind.is_video())
            .filter_map(|(i, t)| {
                let playing = t.clips.iter().find(|c| c.at <= at && at < c.end()).map(|c| c.source_at(at - c.at));
                let ending = || t.clips.iter().find(|c| c.end() == at).map(|c| c.source_at(c.len.saturating_sub(1)));
                playing.or_else(ending).map(|src| (i, src))
            })
            .collect();
        if held.is_empty() {
            return Err(EditError::Invalid("there's no picture to hold there".to_string()));
        }
        self.insert_time(at, len);
        for (i, src) in held {
            let t = self.track_mut(i)?;
            let k = t.clips.partition_point(|c| c.at < at);
            t.clips.insert(k, Clip::held(at, src, len));
        }
        Ok((at, at.saturating_add(len)))
    }

    /// A held frame held for `len` instead (every picture track's hold there with it): longer,
    /// what follows moves along; shorter, it moves up.
    pub fn set_hold(&mut self, track: usize, clip: usize, len: u64) -> Result<(), EditError> {
        let c = *self.track(track)?.clips.get(clip).ok_or(EditError::NoClip(track, clip))?;
        if !c.hold {
            return Err(EditError::Invalid("that clip isn't a held frame".to_string()));
        }
        let len = len.clamp(MIN_CLIP_US, MAX_HOLD_US);
        let (a, e) = (c.at, c.end());
        if len > c.len {
            let more = len - c.len;
            self.insert_time(e, more);
            for t in &mut self.tracks {
                for h in t.clips.iter_mut().filter(|h| h.hold && h.at == a && h.end() == e) {
                    h.len = len;
                }
            }
        } else if len < c.len {
            self.cut(a.saturating_add(len), e)?;
        }
        Ok(())
    }

    /// Takes `from..to` out of one track (or every track) and leaves the gap: silence, or no
    /// picture, there; nothing after it moves, so the other tracks stay in time with it.
    pub fn lift(&mut self, from: u64, to: u64, track: Option<usize>) -> Result<(), EditError> {
        if to <= from {
            return Err(EditError::Invalid("the stretch to remove is empty".to_string()));
        }
        let which: Vec<usize> = match track {
            Some(i) => {
                self.track(i)?;
                vec![i]
            }
            None => (0..self.tracks.len()).collect(),
        };
        for i in which {
            let t = self.track_mut(i)?;
            let mut out = Vec::with_capacity(t.clips.len() + 1);
            for c in &t.clips {
                if c.end() <= from || c.at >= to {
                    out.push(*c);
                    continue;
                }
                // The part before the stretch, and the part after it, where it was.
                if c.at < from {
                    out.push(c.head(from - c.at));
                }
                if c.end() > to {
                    out.push(c.tail(to - c.at));
                }
            }
            out.retain(|c| c.len >= MIN_CLIP_US);
            t.clips = out;
        }
        Ok(())
    }

    /// Splits the clips playing at `at` in two: on one track, or on all of them.
    pub fn split(&mut self, at: u64, track: Option<usize>) -> Result<usize, EditError> {
        let which: Vec<usize> = match track {
            Some(i) => {
                self.track(i)?;
                vec![i]
            }
            None => (0..self.tracks.len()).collect(),
        };
        let mut made = 0;
        for i in which {
            let t = self.track_mut(i)?;
            if let Some(k) = t.clips.iter().position(|c| c.at + MIN_CLIP_US <= at && at + MIN_CLIP_US <= c.end()) {
                let c = t.clips[k];
                let first = at - c.at;
                t.clips[k] = c.head(first);
                t.clips.insert(k + 1, c.tail(first));
                made += 1;
            }
        }
        Ok(made)
    }

    /// Removes one clip, leaving silence (or no picture) where it was.
    pub fn delete_clip(&mut self, track: usize, clip: usize) -> Result<(), EditError> {
        let t = self.track_mut(track)?;
        if clip >= t.clips.len() {
            return Err(EditError::NoClip(track, clip));
        }
        t.clips.remove(clip);
        Ok(())
    }

    fn clip_mut(&mut self, track: usize, clip: usize) -> Result<(&mut Vec<Clip>, Clip), EditError> {
        let t = self.track_mut(track)?;
        let c = *t.clips.get(clip).ok_or(EditError::NoClip(track, clip))?;
        Ok((&mut t.clips, c))
    }

    /// Moves a clip's start to timeline time `to` (later trims it, earlier brings back what was
    /// trimmed), up to the clip before and the start of the recording.
    pub fn trim_start(&mut self, track: usize, clip: usize, to: u64) -> Result<(), EditError> {
        let (clips, c) = self.clip_mut(track, clip)?;
        let before = clip.checked_sub(1).and_then(|p| clips.get(p)).map(Clip::end).unwrap_or(0);
        if c.hold {
            // A held frame holds the same picture longer or shorter.
            let floor = before.max(c.end().saturating_sub(MAX_HOLD_US));
            let to = to.clamp(floor, c.end().saturating_sub(MIN_CLIP_US).max(floor));
            if let Some(slot) = clips.get_mut(clip) {
                *slot = Clip::held(to, c.from, c.end().saturating_sub(to));
            }
            return Ok(());
        }
        // Not before the clip before, nor before where the recording's start would play.
        let floor = before.max(c.at.saturating_sub(c.played(c.from)));
        let to = to.clamp(floor, c.end().saturating_sub(MIN_CLIP_US).max(floor));
        let from = if to >= c.at { c.source_at(to - c.at) } else { c.from.saturating_sub(scale(c.at - to, c.pace(), 100)) };
        let moved = Clip { at: to, from, len: c.end().saturating_sub(to), ..c };
        if let Some(slot) = clips.get_mut(clip) {
            *slot = moved;
        }
        Ok(())
    }

    /// Moves a clip's end to timeline time `to`, up to the clip after and the end of what was
    /// recorded (`source` µs).
    pub fn trim_end(&mut self, track: usize, clip: usize, to: u64, source: u64) -> Result<(), EditError> {
        let (clips, c) = self.clip_mut(track, clip)?;
        let next = clips.get(clip + 1).map(|n| n.at).unwrap_or(u64::MAX);
        let most = if c.hold { MAX_HOLD_US } else { c.played(source.saturating_sub(c.from)) };
        let ceiling = next.min(c.at.saturating_add(most));
        let to = to.clamp(c.at + MIN_CLIP_US, ceiling.max(c.at + MIN_CLIP_US));
        if let Some(slot) = clips.get_mut(clip) {
            slot.len = to - c.at;
        }
        Ok(())
    }

    /// Slides a clip along the timeline to start at `to`, between its neighbours.
    pub fn move_clip(&mut self, track: usize, clip: usize, to: u64) -> Result<(), EditError> {
        let (clips, c) = self.clip_mut(track, clip)?;
        let floor = clip.checked_sub(1).and_then(|p| clips.get(p)).map(Clip::end).unwrap_or(0);
        let ceiling = clips.get(clip + 1).map(|n| n.at.saturating_sub(c.len)).unwrap_or(u64::MAX / 4);
        if ceiling < floor {
            return Ok(());
        }
        if let Some(slot) = clips.get_mut(clip) {
            slot.at = to.clamp(floor, ceiling);
        }
        Ok(())
    }

    /// Plays `from..to` of the timeline at `speed` (hundredths of as recorded, see
    /// [`NORMAL_SPEED`]) on one track, or every track and the layers with them; what comes after
    /// moves up (or along) to follow on. A part of the stretch already faster or slower than its
    /// first clip stays so by as much, so tracks in step stay in step. Returns where the stretch
    /// now ends.
    pub fn set_speed(&mut self, from: u64, to: u64, speed: u16, track: Option<usize>) -> Result<u64, EditError> {
        if to <= from {
            return Err(EditError::Invalid("the stretch to speed up or slow down is empty".to_string()));
        }
        let speed = u64::from(speed.clamp(MIN_SPEED, MAX_SPEED));
        let which: Vec<usize> = match track {
            Some(i) => {
                self.track(i)?;
                vec![i]
            }
            None => (0..self.tracks.len()).collect(),
        };
        // The speed the stretch plays at now: that of its first clip.
        let was = which
            .iter()
            .filter_map(|&i| self.tracks.get(i))
            .flat_map(|t| t.clips.iter())
            .filter(|c| c.at < to && c.end() > from)
            .min_by_key(|c| c.at)
            .map(Clip::pace)
            .ok_or_else(|| EditError::Invalid("there's nothing to speed up or slow down there".to_string()))?;
        let len = to - from;
        let new_len = scale(len, was, speed).max(1);
        // Where a moment of the timeline goes: before the stretch it stays, in it it's sped up or
        // slowed down, after it it moves by as much as the stretch's length changed.
        let map = |t: u64| -> u64 {
            if t <= from {
                t
            } else if t >= to {
                (t - len).saturating_add(new_len)
            } else {
                from.saturating_add(scale(t - from, was, speed))
            }
        };
        for i in which {
            let t = self.track_mut(i)?;
            let mut out = Vec::with_capacity(t.clips.len() + 2);
            for c in &t.clips {
                if c.end() <= from {
                    out.push(*c);
                    continue;
                }
                if c.at >= to {
                    out.push(Clip { at: map(c.at), ..*c });
                    continue;
                }
                if c.at < from {
                    out.push(c.head(from - c.at));
                }
                // The part in the stretch, at its new speed (with its fades, if they're in it).
                let (a, b) = (c.at.max(from), c.end().min(to));
                let pace = scale(c.pace(), speed, was).clamp(u64::from(MIN_SPEED), u64::from(MAX_SPEED));
                let (na, nb) = (map(a), map(b));
                let part = c.tail(a - c.at).head(b - a);
                out.push(Clip { at: na, len: nb.saturating_sub(na), speed: u16::try_from(pace).unwrap_or(MAX_SPEED), ..part });
                if c.end() > to {
                    let mut after = c.tail(to - c.at);
                    after.at = map(to);
                    out.push(after);
                }
            }
            // Apart (rounding may leave a microsecond over), and no slivers.
            let mut kept: Vec<Clip> = Vec::with_capacity(out.len());
            for c in out {
                if let Some(l) = kept.last_mut()
                    && l.end() > c.at
                {
                    l.len = c.at.saturating_sub(l.at);
                }
                kept.push(c);
            }
            kept.retain(|c| c.len >= MIN_CLIP_US);
            t.clips = kept;
        }
        if track.is_none() {
            self.retime_layers(&map);
            for m in &mut self.markers {
                m.at = map(m.at);
            }
            self.markers.dedup_by_key(|m| m.at);
        }
        Ok(map(to))
    }

    /// How long a sound clip's sound takes to come up at its start (`fade_in`) and go down at its
    /// end (`fade_out`), µs; either left as it is if not given. Together they're never longer
    /// than the clip: the one given has its way (both given, the fade in).
    pub fn set_fade(&mut self, track: usize, clip: usize, fade_in: Option<u64>, fade_out: Option<u64>) -> Result<(), EditError> {
        if self.track(track)?.kind.is_video() {
            return Err(EditError::Invalid("only sound fades in and out (a picture fades with a transition from the Library)".to_string()));
        }
        let (clips, mut c) = self.clip_mut(track, clip)?;
        let cap = |v: u64| v.min(MAX_FADE_US).min(c.len);
        match (fade_in, fade_out) {
            (Some(i), Some(o)) => {
                c.fade_in = cap(i);
                c.fade_out = cap(o).min(c.len - c.fade_in);
            }
            (Some(i), None) => {
                c.fade_in = cap(i);
                c.fade_out = c.fade_out.min(c.len - c.fade_in);
            }
            (None, Some(o)) => {
                c.fade_out = cap(o);
                c.fade_in = c.fade_in.min(c.len - c.fade_out);
            }
            (None, None) => {}
        }
        if let Some(slot) = clips.get_mut(clip) {
            *slot = c;
        }
        Ok(())
    }

    /// How much of a sound track's background noise is taken out: 0 none, to 1 the most.
    pub fn set_denoise(&mut self, track: usize, amount: f32) -> Result<(), EditError> {
        if !amount.is_finite() {
            return Err(EditError::Invalid("how much noise to take out is not a number".to_string()));
        }
        let t = self.track_mut(track)?;
        if t.kind.is_video() {
            return Err(EditError::Invalid("only a sound track has noise to take out".to_string()));
        }
        t.denoise = amount.clamp(0.0, 1.0);
        Ok(())
    }

    /// Marks a moment, named (else "Chapter n"): its place among the markers, which stay in order.
    pub fn add_marker(&mut self, at: u64, name: Option<&str>) -> Result<usize, EditError> {
        if self.markers.len() >= MAX_MARKERS {
            return Err(EditError::Invalid(format!("a video has at most {MAX_MARKERS} markers")));
        }
        let at = at.min(u64::MAX / 4);
        if self.markers.iter().any(|m| m.at == at) {
            return Err(EditError::Invalid("there's a marker there already".to_string()));
        }
        let name = marker_name(name.unwrap_or(""), self.markers.len() + 1);
        self.markers.push(Marker { at, name });
        self.markers.sort_by_key(|m| m.at);
        Ok(self.markers.iter().position(|m| m.at == at).unwrap_or(0))
    }

    fn marker_mut(&mut self, i: usize) -> Result<&mut Marker, EditError> {
        self.markers.get_mut(i).ok_or_else(|| EditError::Invalid(format!("there's no marker {i}")))
    }

    pub fn remove_marker(&mut self, i: usize) -> Result<(), EditError> {
        self.marker_mut(i)?;
        self.markers.remove(i);
        Ok(())
    }

    /// A marker's new name (an empty one keeps the old).
    pub fn rename_marker(&mut self, i: usize, name: &str) -> Result<(), EditError> {
        let m = self.marker_mut(i)?;
        if let Some(n) = clean_name(name) {
            m.name = n;
        }
        Ok(())
    }

    /// A marker moved to another moment (not onto another marker): its new place among them.
    pub fn move_marker(&mut self, i: usize, at: u64) -> Result<usize, EditError> {
        let at = at.min(u64::MAX / 4);
        if self.markers.iter().enumerate().any(|(k, m)| k != i && m.at == at) {
            return Err(EditError::Invalid("there's a marker there already".to_string()));
        }
        self.marker_mut(i)?.at = at;
        self.markers.sort_by_key(|m| m.at);
        Ok(self.markers.iter().position(|m| m.at == at).unwrap_or(0))
    }

    /// The chapters of the video made: each marker in it with its name, and a "Start" from the
    /// beginning when the first marker is later (players expect the first chapter at the start).
    /// None when there are no markers.
    pub fn chapters(&self) -> Vec<(u64, String)> {
        let length = self.length();
        let mut out: Vec<(u64, String)> = self.markers.iter().filter(|m| m.at < length).map(|m| (m.at, m.name.clone())).collect();
        if out.first().is_some_and(|(at, _)| *at > 0) {
            out.insert(0, (0, "Start".to_string()));
        }
        out.truncate(MAX_MARKERS);
        out
    }

    pub fn set_muted(&mut self, track: usize, muted: bool) -> Result<(), EditError> {
        self.track_mut(track)?.muted = muted;
        Ok(())
    }

    /// Names a track (an empty name: what it is again).
    pub fn rename_track(&mut self, track: usize, name: &str) -> Result<(), EditError> {
        self.track_mut(track)?.name = clean_name(name);
        Ok(())
    }

    /// Moves a track to another place among the tracks (the timeline's rows, top first).
    pub fn move_track(&mut self, track: usize, to: usize) -> Result<(), EditError> {
        if track >= self.tracks.len() {
            return Err(EditError::NoTrack(track));
        }
        let t = self.tracks.remove(track);
        let to = to.min(self.tracks.len());
        self.tracks.insert(to, t);
        Ok(())
    }

    pub fn set_volume(&mut self, track: usize, volume: f32) -> Result<(), EditError> {
        if !volume.is_finite() {
            return Err(EditError::Invalid("the volume is not a number".to_string()));
        }
        self.track_mut(track)?.volume = volume.clamp(0.0, 2.0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_tracks() -> Timeline {
        let full =
            |stream, kind| Track { stream, kind, muted: false, volume: 1.0, clips: vec![Clip::new(0, 0, 10_000_000)], name: None, denoise: 0.0 };
        Timeline {
            tracks: vec![full(0, StreamKind::Screen), full(1, StreamKind::Microphone)],
            camera: CameraLayout::default(),
            captions: CaptionOptions::default(),
            layers: Vec::new(),
            media: Vec::new(),
            pointer: Default::default(),
            corrected: None,
            markers: Vec::new(),
        }
    }

    #[test]
    fn a_cut_closes_the_gap_on_every_track_and_keeps_them_in_step() {
        let mut t = two_tracks();
        t.cut(2_000_000, 5_000_000).unwrap();
        assert_eq!(t.length(), 7_000_000);
        for track in &t.tracks {
            assert_eq!(track.clips, vec![Clip::new(0, 0, 2_000_000), Clip::new(2_000_000, 5_000_000, 5_000_000)]);
        }
        // What plays at 3 s now is what was recorded at 6 s, on both tracks.
        assert_eq!(t.source_time(0, 3_000_000), Some(6_000_000));
        assert_eq!(t.source_time(1, 3_000_000), Some(6_000_000));
    }

    #[test]
    fn cutting_across_a_gap_and_off_the_end_works() {
        let mut t = two_tracks();
        t.delete_clip(1, 0).unwrap();
        t.cut(8_000_000, 20_000_000).unwrap();
        assert_eq!(t.length(), 8_000_000);
        assert!(t.tracks[1].clips.is_empty());
        assert!(t.cut(5, 5).is_err());
    }

    #[test]
    fn tracks_are_named_and_moved_and_names_are_kept_clean() {
        let mut t = two_tracks();
        t.rename_track(1, "  Bill\u{7}\n ").unwrap();
        assert_eq!(t.tracks[1].name.as_deref(), Some("Bill"));
        t.rename_track(1, "   ").unwrap();
        assert_eq!(t.tracks[1].name, None, "an empty name: what it is again");
        t.rename_track(0, &"x".repeat(500)).unwrap();
        assert_eq!(t.tracks[0].name.as_ref().map(|n| n.chars().count()), Some(MAX_NAME));
        assert!(t.rename_track(7, "a").is_err());
        // The microphone moved to the top; past the end is the bottom; a missing one is an error.
        t.move_track(1, 0).unwrap();
        assert_eq!(t.tracks.iter().map(|t| t.kind).collect::<Vec<_>>(), vec![StreamKind::Microphone, StreamKind::Screen]);
        t.move_track(0, 99).unwrap();
        assert_eq!(t.tracks[1].kind, StreamKind::Microphone);
        assert!(t.move_track(5, 0).is_err());
    }

    #[test]
    fn lifting_a_stretch_leaves_a_gap_and_moves_nothing() {
        let mut t = two_tracks();
        // From the microphone only: the screen plays on, and the sound after it is still in time.
        t.lift(2_000_000, 5_000_000, Some(1)).unwrap();
        assert_eq!(t.tracks[1].clips, vec![Clip::new(0, 0, 2_000_000), Clip::new(5_000_000, 5_000_000, 5_000_000)]);
        assert_eq!(t.source_time(1, 3_000_000), None);
        assert_eq!(t.source_time(1, 6_000_000), Some(6_000_000));
        assert_eq!(t.tracks[0].clips, vec![Clip::new(0, 0, 10_000_000)]);
        assert_eq!(t.length(), 10_000_000);
        // From every track, off the end, over the gap already there: nothing left after 8 s.
        t.lift(8_000_000, u64::MAX, None).unwrap();
        assert_eq!(t.length(), 8_000_000);
        assert_eq!(t.source_time(0, 7_000_000), Some(7_000_000));
        // A whole clip inside the stretch goes; an empty stretch and a missing track are errors.
        t.lift(0, 2_000_000, Some(1)).unwrap();
        assert_eq!(t.tracks[1].clips.first().map(|c| c.at), Some(5_000_000));
        assert!(t.lift(5, 5, None).is_err());
        assert!(t.lift(0, 5, Some(9)).is_err());
    }

    #[test]
    fn split_then_delete_leaves_a_hole_on_one_track_only() {
        let mut t = two_tracks();
        assert_eq!(t.split(3_000_000, Some(1)).unwrap(), 1);
        assert_eq!(t.split(4_000_000, Some(1)).unwrap(), 1);
        t.delete_clip(1, 1).unwrap();
        assert_eq!(t.source_time(1, 3_500_000), None);
        assert_eq!(t.source_time(1, 4_500_000), Some(4_500_000));
        assert_eq!(t.source_time(0, 3_500_000), Some(3_500_000));
        assert_eq!(t.length(), 10_000_000);
        // Splitting right at an edge makes nothing.
        assert_eq!(t.split(4_000_000, Some(1)).unwrap(), 0);
    }

    #[test]
    fn trims_stay_inside_what_was_recorded_and_off_their_neighbours() {
        let mut t = two_tracks();
        t.split(5_000_000, Some(0)).unwrap();
        t.trim_end(0, 0, 9_000_000, 10_000_000).unwrap();
        assert_eq!(t.tracks[0].clips[0].end(), 5_000_000, "stops at the next clip");
        t.trim_start(0, 1, 6_000_000).unwrap();
        assert_eq!(t.tracks[0].clips[1], Clip::new(6_000_000, 6_000_000, 4_000_000));
        t.trim_start(0, 1, 0).unwrap();
        assert_eq!(t.tracks[0].clips[1].at, 5_000_000, "stops at the clip before");
        t.trim_end(0, 1, 99_000_000, 10_000_000).unwrap();
        assert_eq!(t.tracks[0].clips[1].end(), 10_000_000, "stops where the recording does");
    }

    #[test]
    fn moving_a_clip_stops_at_its_neighbours() {
        let mut t = two_tracks();
        t.split(4_000_000, Some(1)).unwrap();
        t.trim_end(1, 0, 3_000_000, 10_000_000).unwrap();
        t.move_clip(1, 0, 500_000).unwrap();
        assert_eq!(t.tracks[1].clips[0].at, 500_000);
        t.move_clip(1, 0, 9_000_000).unwrap();
        assert_eq!(t.tracks[1].clips[0].end(), 4_000_000);
    }

    #[test]
    fn bad_numbers_and_missing_things_are_errors() {
        let mut t = two_tracks();
        assert_eq!(t.delete_clip(5, 0), Err(EditError::NoTrack(5)));
        assert_eq!(t.delete_clip(0, 9), Err(EditError::NoClip(0, 9)));
        assert!(t.set_volume(0, f32::NAN).is_err());
        t.set_volume(1, 9.0).unwrap();
        assert_eq!(t.tracks[1].volume, 2.0);
    }

    #[test]
    fn speeding_up_a_stretch_moves_what_follows_up_and_keeps_the_tracks_in_step() {
        let mut t = two_tracks();
        // 2 s to 6 s twice as fast: it takes 2 s, the rest comes 2 s sooner.
        assert_eq!(t.set_speed(2_000_000, 6_000_000, 200, None).unwrap(), 4_000_000);
        assert_eq!(t.length(), 8_000_000);
        for track in &t.tracks {
            assert_eq!(
                track.clips,
                vec![
                    Clip::new(0, 0, 2_000_000),
                    Clip { speed: 200, ..Clip::new(2_000_000, 2_000_000, 2_000_000) },
                    Clip::new(4_000_000, 6_000_000, 4_000_000)
                ]
            );
        }
        // At 3 s the video shows (and plays) what was recorded at 4 s, on both tracks.
        assert_eq!(t.source_time(0, 3_000_000), Some(4_000_000));
        assert_eq!(t.source_time(1, 3_000_000), Some(4_000_000));
        assert_eq!(t.source_time(1, 5_000_000), Some(7_000_000));
        // Split in the fast part: the second half starts where the recording has got to.
        t.split(3_000_000, None).unwrap();
        assert_eq!(t.tracks[0].clips[2], Clip { speed: 200, ..Clip::new(3_000_000, 4_000_000, 1_000_000) });
        // Back as recorded: as long as it was, nothing lost.
        assert_eq!(t.set_speed(2_000_000, 4_000_000, 100, None).unwrap(), 6_000_000);
        assert_eq!(t.length(), 10_000_000);
        assert_eq!(t.source_time(0, 5_500_000), Some(5_500_000));
        assert_eq!(t.source_time(1, 9_000_000), Some(9_000_000));
    }

    #[test]
    fn slowing_down_makes_the_video_longer_and_trims_follow_the_speed() {
        let mut t = two_tracks();
        assert_eq!(t.set_speed(0, 10_000_000, 50, None).unwrap(), 20_000_000);
        assert_eq!(t.length(), 20_000_000);
        assert_eq!(t.source_time(0, 10_000_000), Some(5_000_000));
        // Trimmed by 2 s on the timeline is 1 s of the recording; trimmed out, it stops where the
        // recording does (20 s at half speed).
        t.trim_end(0, 0, 18_000_000, 10_000_000).unwrap();
        assert_eq!(t.tracks[0].clips[0].source_len(), 9_000_000);
        t.trim_end(0, 0, 99_000_000, 10_000_000).unwrap();
        assert_eq!(t.tracks[0].clips[0].end(), 20_000_000);
        t.trim_start(0, 0, 4_000_000).unwrap();
        assert_eq!(t.tracks[0].clips[0], Clip { speed: 50, ..Clip::new(4_000_000, 2_000_000, 16_000_000) });
        t.trim_start(0, 0, 0).unwrap();
        assert_eq!(t.tracks[0].clips[0], Clip { speed: 50, ..Clip::new(0, 0, 20_000_000) });
    }

    #[test]
    fn one_track_alone_and_parts_already_faster_stay_so() {
        let mut t = two_tracks();
        // The microphone alone, 2 s to 6 s twice as fast: its sound after comes 2 s sooner, the
        // screen doesn't move.
        t.set_speed(2_000_000, 6_000_000, 200, Some(1)).unwrap();
        assert_eq!(t.tracks[0].clips, vec![Clip::new(0, 0, 10_000_000)]);
        assert_eq!(t.tracks[1].clips.last().map(Clip::end), Some(8_000_000));
        // Then the whole video at half speed: the screen as the reference; the microphone's fast
        // part is now as recorded, twice the half.
        t.set_speed(0, 10_000_000, 50, None).unwrap();
        assert_eq!(t.tracks[0].clips[0].speed, 50);
        assert_eq!(t.tracks[1].clips.iter().map(|c| c.speed).collect::<Vec<_>>(), vec![50, 100, 50]);
        assert_eq!(t.tracks[1].clips.last().map(Clip::end), Some(16_000_000));
        // Nothing there, an empty stretch and a missing track are errors and change nothing.
        let before = t.clone();
        assert!(t.set_speed(30_000_000, 40_000_000, 200, None).is_err());
        assert!(t.set_speed(5, 5, 200, None).is_err());
        assert!(t.set_speed(0, 5, 200, Some(7)).is_err());
        assert_eq!(t, before);
        // Out of range speeds are kept in range.
        let mut t = two_tracks();
        t.set_speed(0, 10_000_000, 0, None).unwrap();
        assert_eq!(t.tracks[0].clips[0].speed, MIN_SPEED);
        assert_eq!(t.length(), 100_000_000);
    }

    #[test]
    fn items_on_the_layers_go_with_a_speed_change() {
        use crate::layers::Item;
        let mut t = two_tracks();
        let (layer, inside) = t.place(Item { len: 2_000_000, ..Item::default() }, None, 3_000_000).unwrap();
        let (_, after) = t.place(Item { len: 1_000_000, ..Item::default() }, Some(layer), 7_000_000).unwrap();
        t.set_speed(2_000_000, 6_000_000, 200, None).unwrap();
        let at = |id| t.item(layer, id).map(|i| (i.at, i.len)).unwrap();
        assert_eq!(at(inside), (2_500_000, 1_000_000), "sped up with the stretch");
        assert_eq!(at(after), (5_000_000, 1_000_000), "moved up with what follows");
        // One track alone leaves the layers be.
        t.set_speed(0, 1_000_000, 50, Some(0)).unwrap();
        assert_eq!(t.item(layer, after).map(|i| i.at), Some(5_000_000));
    }

    #[test]
    fn edits_saved_before_speeds_play_as_recorded_and_junk_speeds_are_kept_in_range() {
        let old = r#"{"tracks":[{"stream":0,"kind":"screen","clips":[{"at":0,"from":0,"len":4000000}]}]}"#;
        let t: Timeline = serde_json::from_str(old).unwrap();
        assert_eq!(t.tracks[0].clips[0].speed, NORMAL_SPEED);
        // Written again, a clip as recorded says nothing of its speed; a faster one does.
        assert!(!serde_json::to_string(&t).unwrap().contains("speed"));
        let fast = Clip { speed: 250, ..Clip::new(0, 0, 1) };
        assert!(serde_json::to_string(&fast).unwrap().contains("\"speed\":250"));
        // Speeds out of range from a file are kept in range, and nothing plays past the recording.
        let rec = {
            use crate::recording::{Header, RecordingWriter, StreamInfo};
            let streams = vec![StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 8, height: 8, channels: 0 }];
            let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
            for f in 0..40u64 {
                w.video_frame(0, f * 100_000, &screengrabcraft_model::Raster::filled(8, 8, screengrabcraft_model::Color::BLACK).unwrap()).unwrap();
            }
            Recording::open(Box::new(w.finish(4_000_000).unwrap())).unwrap()
        };
        let junk = r#"{"tracks":[{"stream":0,"kind":"screen","clips":[{"at":0,"from":0,"len":9000000,"speed":0},{"at":9000000,"from":0,"len":9000000,"speed":65535}]}]}"#;
        let t = serde_json::from_str::<Timeline>(junk).unwrap().checked(&rec);
        let clips = &t.tracks[0].clips;
        assert_eq!((clips[0].speed, clips[1].speed), (MIN_SPEED, MAX_SPEED));
        assert_eq!(clips[0].len, 9_000_000, "4 s at a tenth of the speed is 40 s");
        assert_eq!(clips[1].len, 400_000, "4 s at ten times is 0.4 s");
        // A held frame from a file: of a moment recorded, no longer than a hold can be.
        let held = r#"{"tracks":[{"stream":0,"kind":"screen","clips":[{"at":0,"from":99000000,"len":999000000000,"hold":true}]}]}"#;
        let t = serde_json::from_str::<Timeline>(held).unwrap().checked(&rec);
        assert_eq!(t.tracks[0].clips, vec![Clip::held(0, 3_999_999, MAX_HOLD_US)]);
    }

    #[test]
    fn fades_stay_with_the_ends_of_a_clip_through_edits() {
        let mut t = two_tracks();
        assert!(t.set_fade(0, 0, Some(1_000_000), None).is_err(), "the screen doesn't fade");
        t.set_fade(1, 0, Some(2_000_000), Some(3_000_000)).unwrap();
        // Longer than the clip, together: the one given now has its way.
        t.set_fade(1, 0, None, Some(9_000_000)).unwrap();
        assert_eq!((t.tracks[1].clips[0].fade_in, t.tracks[1].clips[0].fade_out), (1_000_000, 9_000_000));
        t.set_fade(1, 0, Some(2_000_000), Some(3_000_000)).unwrap();
        // A cut in the middle: the start keeps the fade in, the end the fade out.
        t.cut(4_000_000, 5_000_000).unwrap();
        let c = &t.tracks[1].clips;
        assert_eq!((c[0].fade_in, c[0].fade_out, c[1].fade_in, c[1].fade_out), (2_000_000, 0, 0, 3_000_000));
        // Sped up, the end piece keeps its fade; trimmed, it stays.
        t.set_speed(6_000_000, 9_000_000, 200, None).unwrap();
        assert_eq!(t.tracks[1].clips.last().map(|c| c.fade_out), Some(3_000_000));
        t.trim_start(1, 0, 500_000).unwrap();
        assert_eq!(t.tracks[1].clips[0].fade_in, 2_000_000);
        assert!(t.set_fade(1, 9, Some(1), None).is_err());
        // How loud: silent at the start, full in the middle.
        let c = Clip { fade_in: 1_000_000, fade_out: 1_000_000, ..Clip::new(0, 0, 4_000_000) };
        assert_eq!((c.fade_at(0), c.fade_at(2_000_000), c.fade_at(4_000_000)), (0.0, 1.0, 0.0));
        assert!((c.fade_at(500_000) - 0.5).abs() < 1e-3);
        // Noise taken out: a sound track only, kept in range.
        t.set_denoise(1, 7.0).unwrap();
        assert_eq!(t.tracks[1].denoise, 1.0);
        assert!(t.set_denoise(0, 0.5).is_err() && t.set_denoise(1, f32::NAN).is_err());
        // Written and read again; old edits have neither.
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"fadeIn\"") && json.contains("\"denoise\""));
        assert_eq!(serde_json::from_str::<Timeline>(&json).unwrap(), t);
        let old: Timeline = serde_json::from_str(r#"{"tracks":[{"stream":1,"kind":"microphone","clips":[{"at":0,"from":0,"len":5}]}]}"#).unwrap();
        assert_eq!((old.tracks[0].denoise, old.tracks[0].clips[0].fade_in), (0.0, 0));
    }

    #[test]
    fn markers_stay_in_order_follow_the_edits_and_become_chapters() {
        let mut t = two_tracks();
        assert!(t.chapters().is_empty(), "no markers, no chapters");
        assert_eq!(t.add_marker(6_000_000, Some("Wrap up")).unwrap(), 0);
        assert_eq!(t.add_marker(2_000_000, None).unwrap(), 0);
        assert_eq!(t.markers[0].name, "Chapter 2");
        assert!(t.add_marker(2_000_000, Some("again")).is_err(), "one to a moment");
        t.rename_marker(0, "  The login\npage ").unwrap();
        t.rename_marker(0, "   ").unwrap();
        assert_eq!(t.markers[0].name, "The loginpage", "one clean line; empty keeps it");
        assert_eq!(t.chapters(), vec![(0, "Start".to_string()), (2_000_000, "The loginpage".to_string()), (6_000_000, "Wrap up".to_string())]);
        // Cut before the second: it moves up; one in the cut goes to where it was.
        t.add_marker(4_500_000, Some("Gone")).unwrap();
        t.cut(4_000_000, 5_000_000).unwrap();
        assert_eq!(t.markers.iter().map(|m| m.at).collect::<Vec<_>>(), vec![2_000_000, 4_000_000, 5_000_000]);
        // Twice as fast from 0 to 4 s: the first comes at 1 s, the rest 2 s sooner.
        t.set_speed(0, 4_000_000, 200, None).unwrap();
        assert_eq!(t.markers.iter().map(|m| m.at).collect::<Vec<_>>(), vec![1_000_000, 2_000_000, 3_000_000]);
        // Moved past another: in order again; onto one: refused. Removed; a missing one is an error.
        assert_eq!(t.move_marker(0, 2_500_000).unwrap(), 1);
        assert!(t.move_marker(0, 2_500_000).is_err());
        t.remove_marker(2).unwrap();
        assert!(t.remove_marker(5).is_err() && t.rename_marker(9, "x").is_err());
        // A marker past the end isn't a chapter.
        t.add_marker(99_000_000, None).unwrap();
        assert_eq!(t.chapters().len(), 3);
        // Saved and read back.
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<Timeline>(&json).unwrap(), t);
    }

    #[test]
    fn a_held_frame_shows_one_picture_while_every_track_waits() {
        use crate::layers::Item;
        let mut t = two_tracks();
        t.add_marker(5_000_000, Some("Later")).unwrap();
        let (layer, title) = t.place(Item { len: 2_000_000, ..Item::default() }, None, 2_000_000).unwrap();
        let (_, after) = t.place(Item { len: 1_000_000, ..Item::default() }, Some(layer), 6_000_000).unwrap();
        assert_eq!(t.hold_frame(3_000_000, 2_000_000).unwrap(), (3_000_000, 5_000_000));
        assert_eq!(t.length(), 12_000_000);
        // The screen shows what it had at 3 s all through the hold, then plays on from there.
        assert_eq!(t.source_time(0, 3_000_000), Some(3_000_000));
        assert_eq!(t.source_time(0, 4_900_000), Some(3_000_000));
        assert_eq!(t.source_time(0, 5_500_000), Some(3_500_000));
        assert_eq!(t.tracks[0].clips[1], Clip::held(3_000_000, 3_000_000, 2_000_000));
        // The sound is silent meanwhile and in step after.
        assert_eq!(t.source_time(1, 4_000_000), None);
        assert_eq!(t.source_time(1, 5_500_000), Some(3_500_000));
        // The title over it stays on through it; the one after, and the marker, move along.
        assert_eq!(t.item(layer, title).map(|i| (i.at, i.len)), Some((2_000_000, 4_000_000)));
        assert_eq!(t.item(layer, after).map(|i| i.at), Some(8_000_000));
        assert_eq!(t.markers[0].at, 7_000_000);
        // Held longer: everything after moves along again; shorter: up. Its trims keep its picture.
        t.set_hold(0, 1, 3_000_000).unwrap();
        assert_eq!((t.length(), t.tracks[0].clips[1].len, t.source_time(0, 6_500_000)), (13_000_000, 3_000_000, Some(3_500_000)));
        t.set_hold(0, 1, 500_000).unwrap();
        assert_eq!((t.length(), t.source_time(0, 3_400_000), t.source_time(0, 4_000_000)), (10_500_000, Some(3_000_000), Some(3_500_000)));
        assert!(t.set_hold(0, 0, 1_000_000).is_err() && t.set_hold(1, 1, 1_000_000).is_err(), "not a held frame");
        t.trim_end(0, 1, 9_000_000, 10_000_000).unwrap();
        assert_eq!(t.tracks[0].clips[1].end(), 3_500_000, "up to the clip after");
        t.trim_start(0, 1, 2_000_000).unwrap();
        assert_eq!(t.tracks[0].clips[1].at, 3_000_000, "up to the clip before");
        // Cut through it, split in it, sped up: a held frame stays held.
        t.split(3_200_000, None).unwrap();
        assert!(t.tracks[0].clips[1].hold && t.tracks[0].clips[2].hold && t.source_time(0, 3_300_000) == Some(3_000_000));
        // At the end: the last picture held. Nothing to hold, or no time: errors.
        let end = t.length();
        t.hold_frame(end, 1_000_000).unwrap();
        assert_eq!(t.source_time(0, end + 500_000), Some(9_999_999));
        assert!(t.hold_frame(end + 5_000_000, 1_000_000).is_err() && t.hold_frame(1_000_000, 5).is_err());
        // Saved and read back.
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"hold\":true"));
        assert_eq!(serde_json::from_str::<Timeline>(&json).unwrap(), t);
        assert!(!serde_json::to_string(&two_tracks()).unwrap().contains("hold"), "clips as recorded say nothing of it");
    }

    #[test]
    fn a_stretch_is_a_copy_with_only_it_left_from_the_start() {
        use crate::layers::Item;
        let mut t = two_tracks();
        t.add_marker(1_000_000, Some("Before")).unwrap();
        t.add_marker(4_000_000, Some("In it")).unwrap();
        t.add_marker(8_000_000, Some("After")).unwrap();
        let (layer, title) = t.place(Item { len: 2_000_000, ..Item::default() }, None, 2_000_000).unwrap();
        let s = t.stretch(3_000_000, 6_000_000).unwrap();
        assert_eq!(s.length(), 3_000_000);
        // At 0.5 s it plays what was at 3.5 s, on both tracks.
        assert_eq!((s.source_time(0, 500_000), s.source_time(1, 500_000)), (Some(3_500_000), Some(3_500_000)));
        assert_eq!(s.markers, vec![Marker { at: 1_000_000, name: "In it".into() }]);
        // The title from 2 s to 4 s: its last second is left, from the start.
        assert_eq!(s.item(layer, title).map(|i| (i.at, i.len)), Some((0, 1_000_000)));
        // The edits themselves are as they were.
        assert_eq!(t.length(), 10_000_000);
        assert_eq!(t.markers.len(), 3);
        // To the end, past it, from the start; nothing there is an error.
        assert_eq!(t.stretch(0, 99_000_000).unwrap().length(), 10_000_000);
        assert_eq!(t.stretch(9_000_000, u64::MAX).unwrap().length(), 1_000_000);
        assert!(t.stretch(5, 5).is_err() && t.stretch(20_000_000, 30_000_000).is_err());
    }

    #[test]
    fn edits_survive_being_written_and_read() {
        let mut t = two_tracks();
        t.cut(1_000_000, 2_000_000).unwrap();
        t.camera.shape = CameraShape::Circle;
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<Timeline>(&json).unwrap(), t);
    }
}
