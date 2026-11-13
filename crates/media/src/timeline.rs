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

/// A stretch of a track's recording, placed on the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clip {
    /// Where it starts on the timeline.
    pub at: u64,
    /// Where it starts in the recording.
    pub from: u64,
    pub len: u64,
}

impl Clip {
    pub fn end(&self) -> u64 {
        self.at.saturating_add(self.len)
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
                let clips = if len >= MIN_CLIP_US { vec![Clip { at: 0, from: 0, len }] } else { Vec::new() };
                Track { stream: s.id, kind: s.kind, muted: false, volume: 1.0, clips }
            })
            .collect();
        Timeline {
            tracks,
            camera: CameraLayout::default(),
            captions: CaptionOptions::default(),
            layers: Vec::new(),
            media: Vec::new(),
            pointer: Default::default(),
        }
    }

    /// Edits read from a file, made safe for this recording: tracks it doesn't have go, tracks
    /// it has come back whole, clips are kept inside what was recorded and apart.
    pub fn checked(mut self, rec: &Recording) -> Timeline {
        let fresh = Timeline::new(rec);
        let mut tracks = Vec::new();
        for f in fresh.tracks {
            let found = self.tracks.iter().position(|t| t.stream == f.stream);
            let mut t = match found {
                Some(i) => self.tracks.swap_remove(i),
                None => f.clone(),
            };
            t.kind = f.kind;
            t.volume = if t.volume.is_finite() { t.volume.clamp(0.0, 2.0) } else { 1.0 };
            let source = rec.stream_duration(t.stream);
            let mut clips: Vec<Clip> = t
                .clips
                .iter()
                .filter_map(|c| {
                    let from = c.from.min(source);
                    let len = c.len.min(source - from);
                    (len >= MIN_CLIP_US).then_some(Clip { at: c.at.min(u64::MAX / 4), from, len })
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
        };
        checked.check_layers();
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
        Some(c.from + (t - c.at))
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
                    let len = c.len.min(from - c.at);
                    out.push(Clip { at: c.at, from: c.from, len });
                }
                if c.end() > to {
                    let skip = to.saturating_sub(c.at);
                    out.push(Clip { at: c.at.max(to) - gap, from: c.from + skip, len: c.len - skip });
                }
            }
            out.retain(|c| c.len >= MIN_CLIP_US);
            t.clips = out;
        }
        self.cut_layers(from, to);
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
                t.clips[k].len = first;
                t.clips.insert(k + 1, Clip { at, from: c.from + first, len: c.len - first });
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
        let floor = clip.checked_sub(1).and_then(|p| clips.get(p)).map(Clip::end).unwrap_or(0).max(c.at.saturating_sub(c.from));
        let to = to.clamp(floor, c.end().saturating_sub(MIN_CLIP_US));
        let moved = Clip { at: to, from: (c.from + to).saturating_sub(c.at), len: c.end() - to };
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
        let ceiling = next.min(c.at + source.saturating_sub(c.from));
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

    pub fn set_muted(&mut self, track: usize, muted: bool) -> Result<(), EditError> {
        self.track_mut(track)?.muted = muted;
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
        let full = |stream, kind| Track { stream, kind, muted: false, volume: 1.0, clips: vec![Clip { at: 0, from: 0, len: 10_000_000 }] };
        Timeline {
            tracks: vec![full(0, StreamKind::Screen), full(1, StreamKind::Microphone)],
            camera: CameraLayout::default(),
            captions: CaptionOptions::default(),
            layers: Vec::new(),
            media: Vec::new(),
            pointer: Default::default(),
        }
    }

    #[test]
    fn a_cut_closes_the_gap_on_every_track_and_keeps_them_in_step() {
        let mut t = two_tracks();
        t.cut(2_000_000, 5_000_000).unwrap();
        assert_eq!(t.length(), 7_000_000);
        for track in &t.tracks {
            assert_eq!(track.clips, vec![Clip { at: 0, from: 0, len: 2_000_000 }, Clip { at: 2_000_000, from: 5_000_000, len: 5_000_000 }]);
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
        assert_eq!(t.tracks[0].clips[1], Clip { at: 6_000_000, from: 6_000_000, len: 4_000_000 });
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
    fn edits_survive_being_written_and_read() {
        let mut t = two_tracks();
        t.cut(1_000_000, 2_000_000).unwrap();
        t.camera.shape = CameraShape::Circle;
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<Timeline>(&json).unwrap(), t);
    }
}
