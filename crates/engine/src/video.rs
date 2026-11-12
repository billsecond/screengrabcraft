//! Editing a video recording: a [`VideoSession`] holds the timeline (the edits; the recording
//! itself never changes), undo and redo, and runs the `video.*` commands in [`VIDEO_COMMANDS`].
//!
//! Times in parameters are microseconds; tracks and clips are counted from 0.

use std::collections::BTreeMap;

use screengrabcraft_media::recording::Recording;
use screengrabcraft_media::timeline::{CameraShape, Corner, EditError, Timeline};
use serde_json::{Value, json};

use crate::{EngineError, Result};

/// The most undo steps kept.
const MAX_UNDO: usize = 200;

/// One video editing command.
pub struct VideoCommand {
    pub id: &'static str,
    pub label: &'static str,
    /// What parameters it takes (JSON), for people and agents.
    pub params: &'static str,
    pub run: fn(&mut VideoSession, &Value) -> Result<Value>,
}

pub struct VideoSession {
    pub timeline: Timeline,
    /// The recording as made, for going back to it.
    original: Timeline,
    /// How long each track's recording is, in µs (trims stop there).
    sources: BTreeMap<u8, u64>,
    undo: Vec<(String, Timeline)>,
    redo: Vec<(String, Timeline)>,
    /// A drag under way: its label and the timeline before it.
    pending: Option<(String, Timeline)>,
    /// Changed since it was last saved.
    pub dirty: bool,
    revision: u64,
}

fn edit_err(e: EditError) -> EngineError {
    match e {
        EditError::Invalid(m) => EngineError::Params(m),
        other => EngineError::Failed(other.to_string()),
    }
}

fn int(p: &Value, key: &str) -> Result<u64> {
    let v = p.get(key).ok_or_else(|| EngineError::Params(format!("`{key}` is missing")))?;
    match v.as_u64() {
        Some(n) => Ok(n),
        None => match v.as_f64() {
            Some(f) if f.is_finite() && f >= 0.0 => Ok(f.min(u64::MAX as f64 / 2.0) as u64),
            _ => Err(EngineError::Params(format!("`{key}` must be a number of microseconds, 0 or more"))),
        },
    }
}

fn index(p: &Value, key: &str) -> Result<usize> {
    usize::try_from(int(p, key)?).map_err(|_| EngineError::Params(format!("`{key}` is too large")))
}

impl VideoSession {
    /// The recording's own edits if it has some, else every track whole.
    pub fn new(rec: &Recording) -> VideoSession {
        let original = Timeline::new(rec);
        let timeline = rec.timeline.clone().unwrap_or_else(|| original.clone());
        let sources = rec.header.streams.iter().map(|s| (s.id, rec.stream_duration(s.id))).collect();
        VideoSession { timeline, original, sources, undo: Vec::new(), redo: Vec::new(), pending: None, dirty: false, revision: 0 }
    }

    /// Changes with every edit (for knowing when to save or redraw).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// How long a track's recording is.
    pub fn source_len(&self, track: usize) -> u64 {
        self.timeline.tracks.get(track).and_then(|t| self.sources.get(&t.stream)).copied().unwrap_or(0)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|(l, _)| l.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|(l, _)| l.as_str())
    }

    fn changed(&mut self, label: &str, before: Timeline) {
        if before == self.timeline {
            return;
        }
        self.undo.push((label.to_string(), before));
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.revision += 1;
    }

    /// Runs a command by id: one undo step if it changed anything. A command that fails leaves
    /// the timeline as it was.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value> {
        match id {
            "edit.undo" => return self.step(true),
            "edit.redo" => return self.step(false),
            _ => {}
        }
        let cmd = VIDEO_COMMANDS.iter().find(|c| c.id == id).ok_or_else(|| EngineError::Unknown(id.to_string()))?;
        let before = self.timeline.clone();
        match (cmd.run)(self, params) {
            Ok(v) => {
                self.changed(cmd.label, before);
                Ok(v)
            }
            Err(e) => {
                self.timeline = before;
                Err(e)
            }
        }
    }

    fn step(&mut self, back: bool) -> Result<Value> {
        let (from, to) = if back { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        let Some((label, timeline)) = from.pop() else { return Ok(json!({"changed": false})) };
        let now = std::mem::replace(&mut self.timeline, timeline);
        to.push((label, now));
        self.dirty = true;
        self.revision += 1;
        Ok(json!({"changed": true}))
    }

    /// A drag starts: everything until [`VideoSession::commit`] is one undo step.
    pub fn begin(&mut self, label: &str) {
        if self.pending.is_none() {
            self.pending = Some((label.to_string(), self.timeline.clone()));
        }
    }

    /// A step of a drag (no undo step of its own).
    pub fn drag(&mut self, id: &str, params: &Value) -> Result<Value> {
        let cmd = VIDEO_COMMANDS.iter().find(|c| c.id == id).ok_or_else(|| EngineError::Unknown(id.to_string()))?;
        let before = self.timeline.clone();
        let r = (cmd.run)(self, params);
        if r.is_err() {
            self.timeline = before;
        } else if before != self.timeline {
            self.revision += 1;
        }
        r
    }

    pub fn commit(&mut self) {
        if let Some((label, before)) = self.pending.take() {
            self.changed(&label, before);
        }
    }

    pub fn cancel(&mut self) {
        if let Some((_, before)) = self.pending.take() {
            self.timeline = before;
            self.revision += 1;
        }
    }

    /// Whether the edits differ from the recording as made.
    pub fn edited(&self) -> bool {
        self.timeline != self.original
    }
}

fn cut(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let (from, to) = (int(p, "from")?, int(p, "to")?);
    s.timeline.cut(from, to).map_err(edit_err)?;
    Ok(json!({"length": s.timeline.length()}))
}

fn split(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let at = int(p, "at")?;
    let track = if p.get("track").is_some_and(|t| !t.is_null()) { Some(index(p, "track")?) } else { None };
    let made = s.timeline.split(at, track).map_err(edit_err)?;
    if made == 0 {
        return Err(EngineError::Failed("there's no clip to split there".to_string()));
    }
    Ok(json!({"split": made}))
}

fn delete_clip(s: &mut VideoSession, p: &Value) -> Result<Value> {
    s.timeline.delete_clip(index(p, "track")?, index(p, "clip")?).map_err(edit_err)?;
    Ok(json!({}))
}

fn trim_start(s: &mut VideoSession, p: &Value) -> Result<Value> {
    s.timeline.trim_start(index(p, "track")?, index(p, "clip")?, int(p, "to")?).map_err(edit_err)?;
    Ok(json!({}))
}

fn trim_end(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let track = index(p, "track")?;
    let source = s.source_len(track);
    s.timeline.trim_end(track, index(p, "clip")?, int(p, "to")?, source).map_err(edit_err)?;
    Ok(json!({}))
}

fn move_clip(s: &mut VideoSession, p: &Value) -> Result<Value> {
    s.timeline.move_clip(index(p, "track")?, index(p, "clip")?, int(p, "to")?).map_err(edit_err)?;
    Ok(json!({}))
}

fn mute(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let muted = p.get("muted").and_then(Value::as_bool).ok_or_else(|| EngineError::Params("`muted` must be true or false".to_string()))?;
    s.timeline.set_muted(index(p, "track")?, muted).map_err(edit_err)?;
    Ok(json!({}))
}

fn volume(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let v = p.get("volume").and_then(Value::as_f64).ok_or_else(|| EngineError::Params("`volume` must be a number".to_string()))?;
    s.timeline.set_volume(index(p, "track")?, v as f32).map_err(edit_err)?;
    Ok(json!({}))
}

fn camera(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let c = &mut s.timeline.camera;
    if let Some(v) = p.get("corner") {
        c.corner = serde_json::from_value::<Corner>(v.clone())
            .map_err(|_| EngineError::Params("`corner` is topLeft, topRight, bottomLeft or bottomRight".to_string()))?;
    }
    if let Some(v) = p.get("shape") {
        c.shape = serde_json::from_value::<CameraShape>(v.clone())
            .map_err(|_| EngineError::Params("`shape` is rectangle, rounded or circle".to_string()))?;
    }
    if let Some(v) = p.get("size") {
        let size = v.as_f64().filter(|f| f.is_finite()).ok_or_else(|| EngineError::Params("`size` must be a number".to_string()))?;
        c.size = (size as f32).clamp(0.1, 0.5);
    }
    Ok(json!({}))
}

fn captions(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let flag = |key: &str| -> Result<Option<bool>> {
        match p.get(key) {
            None => Ok(None),
            Some(v) => v.as_bool().map(Some).ok_or_else(|| EngineError::Params(format!("`{key}` must be true or false"))),
        }
    };
    let (show, burn) = (flag("show")?, flag("burn")?);
    let c = &mut s.timeline.captions;
    if let Some(v) = show {
        c.show = v;
    }
    if let Some(v) = burn {
        c.burn = v;
    }
    Ok(json!({"show": c.show, "burn": c.burn}))
}

fn revert(s: &mut VideoSession, _: &Value) -> Result<Value> {
    s.timeline = s.original.clone();
    Ok(json!({}))
}

pub const VIDEO_COMMANDS: &[VideoCommand] = &[
    VideoCommand { id: "video.cut", label: "Cut", params: r#"{"from": µs, "to": µs}: takes the stretch out of every track"#, run: cut },
    VideoCommand { id: "video.split", label: "Split", params: r#"{"at": µs, "track"?: n}: one track, or all"#, run: split },
    VideoCommand { id: "video.deleteClip", label: "Delete Clip", params: r#"{"track": n, "clip": n}: leaves a gap"#, run: delete_clip },
    VideoCommand { id: "video.trimStart", label: "Trim", params: r#"{"track": n, "clip": n, "to": µs}"#, run: trim_start },
    VideoCommand { id: "video.trimEnd", label: "Trim", params: r#"{"track": n, "clip": n, "to": µs}"#, run: trim_end },
    VideoCommand { id: "video.moveClip", label: "Move Clip", params: r#"{"track": n, "clip": n, "to": µs}"#, run: move_clip },
    VideoCommand { id: "video.mute", label: "Mute", params: r#"{"track": n, "muted": bool}"#, run: mute },
    VideoCommand { id: "video.volume", label: "Volume", params: r#"{"track": n, "volume": 0..2}"#, run: volume },
    VideoCommand {
        id: "video.camera",
        label: "Camera",
        params: r#"{"corner"?: "topLeft"|"topRight"|"bottomLeft"|"bottomRight", "shape"?: "rectangle"|"rounded"|"circle", "size"?: 0.1..0.5}"#,
        run: camera,
    },
    VideoCommand {
        id: "video.captions",
        label: "Captions",
        params: r#"{"show"?: bool, "burn"?: bool}: the captions on the picture in the editor, and burned into the video made"#,
        run: captions,
    },
    VideoCommand { id: "video.revert", label: "Revert to Recording", params: "{}: every track whole again", run: revert },
];

#[cfg(test)]
mod tests {
    use screengrabcraft_media::recording::{Header, RecordingWriter, StreamInfo, StreamKind};
    use screengrabcraft_model::{Color, Raster};

    use super::*;

    fn session() -> VideoSession {
        let streams = vec![
            StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 16, height: 16, channels: 0 },
            StreamInfo { id: 2, kind: StreamKind::Microphone, name: String::new(), width: 0, height: 0, channels: 1 },
        ];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        for t in 0..50u64 {
            w.video_frame(0, t * 100_000, &Raster::filled(16, 16, Color::rgb(t as u8, 0, 0)).unwrap()).unwrap();
            w.audio(2, t * 4800, &vec![100; 4800]).unwrap();
        }
        let rec = Recording::open(Box::new(w.finish(5_000_000).unwrap())).unwrap();
        VideoSession::new(&rec)
    }

    #[test]
    fn a_cut_is_one_undo_step_and_redo_puts_it_back() {
        let mut s = session();
        s.execute("video.cut", &json!({"from": 1_000_000, "to": 2_000_000})).unwrap();
        assert_eq!(s.timeline.length(), 4_000_000);
        assert!(s.dirty && s.edited());
        assert_eq!(s.undo_label(), Some("Cut"));
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.timeline.length(), 5_000_000);
        s.execute("edit.redo", &json!({})).unwrap();
        assert_eq!(s.timeline.length(), 4_000_000);
    }

    #[test]
    fn a_drag_is_one_undo_step_and_cancelling_it_puts_things_back() {
        let mut s = session();
        s.execute("video.split", &json!({"at": 2_000_000, "track": 1})).unwrap();
        s.begin("Trim");
        for to in [2_100_000u64, 2_300_000, 2_500_000] {
            s.drag("video.trimStart", &json!({"track": 1, "clip": 1, "to": to})).unwrap();
        }
        s.commit();
        assert_eq!(s.timeline.tracks[1].clips[1].at, 2_500_000);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.timeline.tracks[1].clips[1].at, 2_000_000);
        s.begin("Move Clip");
        s.drag("video.moveClip", &json!({"track": 1, "clip": 1, "to": 3_000_000})).unwrap();
        s.cancel();
        assert_eq!(s.timeline.tracks[1].clips[1].at, 2_000_000);
    }

    #[test]
    fn trims_stop_where_the_recording_does() {
        let mut s = session();
        s.execute("video.trimEnd", &json!({"track": 1, "clip": 0, "to": 99_000_000})).unwrap();
        assert_eq!(s.timeline.tracks[1].clips[0].end(), 5_000_000);
    }

    #[test]
    fn the_camera_mute_volume_and_revert_work() {
        let mut s = session();
        s.execute("video.camera", &json!({"corner": "topLeft", "shape": "circle", "size": 0.9})).unwrap();
        assert_eq!(s.timeline.camera.corner, Corner::TopLeft);
        assert_eq!(s.timeline.camera.size, 0.5);
        s.execute("video.mute", &json!({"track": 1, "muted": true})).unwrap();
        s.execute("video.volume", &json!({"track": 1, "volume": 1.5})).unwrap();
        assert!(s.timeline.tracks[1].muted);
        s.execute("video.revert", &json!({})).unwrap();
        assert!(!s.edited());
    }

    #[test]
    fn the_captions_switches_are_undone_like_any_edit() {
        let mut s = session();
        assert!(s.timeline.captions.show && !s.timeline.captions.burn);
        s.execute("video.captions", &json!({"burn": true})).unwrap();
        s.execute("video.captions", &json!({"show": false})).unwrap();
        assert!(!s.timeline.captions.show && s.timeline.captions.burn);
        assert_eq!(s.undo_label(), Some("Captions"));
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(s.timeline.captions.show && s.timeline.captions.burn);
        // Half right is still all wrong: nothing changes.
        assert!(s.execute("video.captions", &json!({"show": false, "burn": "yes"})).is_err());
        assert!(s.timeline.captions.show);
    }

    #[test]
    fn junk_parameters_are_errors_and_change_nothing() {
        let mut s = session();
        let before = s.timeline.clone();
        let junk = [
            json!({}),
            json!({"from": -5, "to": "x"}),
            json!({"track": 1e30, "clip": -1, "to": f64::MAX}),
            json!({"at": null, "track": "two"}),
            json!({"corner": "middle", "shape": 7, "size": "big", "muted": 3, "volume": null}),
            json!([1, 2, 3]),
            json!(null),
        ];
        for c in VIDEO_COMMANDS.iter().filter(|c| c.id != "video.revert" && c.id != "video.camera") {
            for p in &junk {
                let _ = s.execute(c.id, p);
                assert_eq!(s.timeline, before, "{} {p}", c.id);
            }
        }
        assert!(s.execute("video.camera", &json!({"corner": "middle"})).is_err());
        assert!(s.execute("video.nothing", &json!({})).is_err());
        assert_eq!(s.timeline, before);
    }
}
