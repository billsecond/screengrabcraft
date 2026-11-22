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
    /// The recording's captions as they were heard: what putting them right starts from.
    heard: Vec<screengrabcraft_media::captions::Cue>,
    /// How loud a track plays (dB), as the host measures it from the recording (the engine reads
    /// no sound itself): for evening levels out.
    measure: Option<Measure>,
}

/// Measures how loud track `n` of these edits plays (dB of the full scale), before its volume.
pub type Measure = Box<dyn Fn(&Timeline, usize) -> Option<f32> + Send + Sync>;

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
        let heard = rec.captions.as_ref().map(|c| c.cues.clone()).unwrap_or_default();
        VideoSession {
            timeline,
            original,
            sources,
            undo: Vec::new(),
            redo: Vec::new(),
            pending: None,
            dirty: false,
            revision: 0,
            heard,
            measure: None,
        }
    }

    /// The recording's captions as heard, when they come after it was opened (made meanwhile):
    /// what putting them right starts from. Lines already put right stay so.
    pub fn set_heard(&mut self, captions: Option<&screengrabcraft_media::captions::Captions>) {
        self.heard = captions.map(|c| c.cues.clone()).unwrap_or_default();
    }

    /// How the host measures how loud a track plays (see [`Measure`]): without it, levels can't be
    /// evened out.
    pub fn set_measure(&mut self, m: Measure) {
        self.measure = Some(m);
    }

    /// How long the recording is (µs): its longest track.
    fn longest(&self) -> u64 {
        self.sources.values().copied().max().unwrap_or(0)
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

fn lift(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let (from, to) = (int(p, "from")?, int(p, "to")?);
    let track = if p.get("track").is_some_and(|t| !t.is_null()) { Some(index(p, "track")?) } else { None };
    s.timeline.lift(from, to, track).map_err(edit_err)?;
    Ok(json!({}))
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

/// A stretch played faster or slower: on every track (and the layers), or one.
fn speed(s: &mut VideoSession, p: &Value) -> Result<Value> {
    use screengrabcraft_media::timeline::{MAX_SPEED, MIN_SPEED};
    let (from, to) = (int(p, "from")?, int(p, "to")?);
    let x = p
        .get("speed")
        .and_then(Value::as_f64)
        .filter(|f| f.is_finite() && *f > 0.0)
        .ok_or_else(|| EngineError::Params("`speed` must be a number above 0: 2 is twice as fast, 0.5 half as fast".to_string()))?;
    let hundredths = (x * 100.0).round().clamp(f64::from(MIN_SPEED), f64::from(MAX_SPEED)) as u16;
    let track = if p.get("track").is_some_and(|t| !t.is_null()) { Some(index(p, "track")?) } else { None };
    let end = s.timeline.set_speed(from, to, hundredths, track).map_err(edit_err)?;
    Ok(json!({"from": from, "to": end, "speed": f64::from(hundredths) / 100.0, "length": s.timeline.length()}))
}

/// The picture at a moment held for a while (a freeze frame): every track waits.
fn hold_frame(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let (at, len) = (int(p, "at")?, int(p, "len")?);
    let (from, to) = s.timeline.hold_frame(at, len).map_err(edit_err)?;
    Ok(json!({"from": from, "to": to, "length": s.timeline.length()}))
}

/// A held frame held longer or shorter.
fn hold_length(s: &mut VideoSession, p: &Value) -> Result<Value> {
    s.timeline.set_hold(index(p, "track")?, index(p, "clip")?, int(p, "len")?).map_err(edit_err)?;
    Ok(json!({"length": s.timeline.length()}))
}

/// A sound clip's fades: how long its sound takes to come up at its start and go down at its end.
fn fade(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let (track, clip) = (index(p, "track")?, index(p, "clip")?);
    let given = |key: &str| -> Result<Option<u64>> { if p.get(key).is_some_and(|v| !v.is_null()) { int(p, key).map(Some) } else { Ok(None) } };
    let (fade_in, fade_out) = (given("in")?, given("out")?);
    if fade_in.is_none() && fade_out.is_none() {
        return Err(EngineError::Params("say how long a fade: `in`, `out` or both, in µs".to_string()));
    }
    s.timeline.set_fade(track, clip, fade_in, fade_out).map_err(edit_err)?;
    let c = s.timeline.tracks.get(track).and_then(|t| t.clips.get(clip)).copied().unwrap_or_default();
    Ok(json!({"in": c.fade_in, "out": c.fade_out}))
}

/// How much of a sound track's background noise is taken out.
fn denoise(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let amount = number(p, "amount")?.ok_or_else(|| EngineError::Params("`amount` is missing: 0 takes nothing out, 1 the most".to_string()))?;
    s.timeline.set_denoise(index(p, "track")?, amount).map_err(edit_err)?;
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
    // The media bin stays (it's only what was brought in), and the captions put right; the
    // layers go.
    let media = std::mem::take(&mut s.timeline.media);
    let corrected = s.timeline.corrected.take();
    s.timeline = s.original.clone();
    s.timeline.media = media;
    s.timeline.corrected = corrected;
    Ok(json!({}))
}

// ---- the captions put right (see `screengrabcraft_media::captions::edit`) ----

/// The captions to put right: those already put right, else a copy of those heard.
fn corrected(s: &mut VideoSession) -> &mut Vec<screengrabcraft_media::captions::Cue> {
    let heard = &s.heard;
    s.timeline.corrected.get_or_insert_with(|| heard.clone())
}

fn caption_text(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let (line, words) = (index(p, "line")?, text(p, "text")?.ok_or_else(|| EngineError::Params("`text` is missing".to_string()))?);
    screengrabcraft_media::captions::edit::text(corrected(s), line, &words).map_err(EngineError::Failed)?;
    Ok(json!({}))
}

fn caption_split(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let line = index(p, "line")?;
    let at = if p.get("at").is_some_and(|v| !v.is_null()) { Some(index(p, "at")?) } else { None };
    screengrabcraft_media::captions::edit::split(corrected(s), line, at).map_err(EngineError::Failed)?;
    Ok(json!({}))
}

fn caption_join(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let line = index(p, "line")?;
    screengrabcraft_media::captions::edit::join(corrected(s), line).map_err(EngineError::Failed)?;
    Ok(json!({}))
}

fn caption_times(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let line = index(p, "line")?;
    let given = |key: &str| -> Result<Option<u64>> { if p.get(key).is_some_and(|v| !v.is_null()) { int(p, key).map(Some) } else { Ok(None) } };
    let (start, end) = (given("start")?, given("end")?);
    if start.is_none() && end.is_none() {
        return Err(EngineError::Params("say where the line goes: `start`, `end` or both, in µs of the recording".to_string()));
    }
    let longest = s.longest();
    screengrabcraft_media::captions::edit::times(corrected(s), line, start, end, longest).map_err(EngineError::Failed)?;
    Ok(json!({}))
}

// ---- markers: moments marked on the timeline, the chapters of the video made ----

fn add_marker(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let name = text(p, "name")?;
    let i = s.timeline.add_marker(int(p, "at")?, name.as_deref()).map_err(edit_err)?;
    Ok(json!({"marker": i}))
}

fn remove_marker(s: &mut VideoSession, p: &Value) -> Result<Value> {
    s.timeline.remove_marker(index(p, "marker")?).map_err(edit_err)?;
    Ok(json!({}))
}

fn rename_marker(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let name = text(p, "name")?.ok_or_else(|| EngineError::Params("`name` is missing".to_string()))?;
    s.timeline.rename_marker(index(p, "marker")?, &name).map_err(edit_err)?;
    Ok(json!({}))
}

fn move_marker(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let i = s.timeline.move_marker(index(p, "marker")?, int(p, "at")?).map_err(edit_err)?;
    Ok(json!({"marker": i}))
}

fn captions_heard(s: &mut VideoSession, _: &Value) -> Result<Value> {
    s.timeline.corrected = None;
    Ok(json!({}))
}

// ---- the layers: media, titles, overlays, transitions (see `screengrabcraft_media::layers`) ----

fn id(p: &Value, key: &str) -> Result<u32> {
    let v = int(p, key)?;
    u32::try_from(v).map_err(|_| EngineError::Params(format!("`{key}` is too large")))
}

/// How the mouse pointer is drawn on the video: shown or not, how large, how smooth its path,
/// whether its clicks are ringed (and heard), and whether it's highlighted, spotlit or magnified.
fn pointer(s: &mut VideoSession, p: &Value) -> Result<Value> {
    if !p.is_object() {
        return Err(EngineError::Params("the pointer's look is given as an object".to_string()));
    }
    let flag = |key: &str| -> Result<Option<bool>> {
        match p.get(key) {
            None => Ok(None),
            Some(v) => v.as_bool().map(Some).ok_or_else(|| EngineError::Params(format!("`{key}` must be true or false"))),
        }
    };
    let (show, clicks, size, smooth) = (flag("show")?, flag("clicks")?, number(p, "size")?, number(p, "smooth")?);
    let (highlight, spotlight, magnify, click_sound) = (flag("highlight")?, flag("spotlight")?, flag("magnify")?, flag("clickSound")?);
    let mut look = s.timeline.pointer;
    look.show = show.unwrap_or(look.show);
    look.clicks = clicks.unwrap_or(look.clicks);
    look.highlight = highlight.unwrap_or(look.highlight);
    look.spotlight = spotlight.unwrap_or(look.spotlight);
    look.magnify = magnify.unwrap_or(look.magnify);
    look.click_sound = click_sound.unwrap_or(look.click_sound);
    look.size = size.unwrap_or(look.size);
    look.smooth = smooth.unwrap_or(look.smooth);
    s.timeline.pointer = look.checked();
    Ok(json!({}))
}

/// How the shortcuts pressed while recording are shown on the video.
fn keys(s: &mut VideoSession, p: &Value) -> Result<Value> {
    use screengrabcraft_media::keys::Place;
    if !p.is_object() {
        return Err(EngineError::Params("how the keys look is given as an object".to_string()));
    }
    let flag = |key: &str| -> Result<Option<bool>> {
        match p.get(key) {
            None => Ok(None),
            Some(v) => v.as_bool().map(Some).ok_or_else(|| EngineError::Params(format!("`{key}` must be true or false"))),
        }
    };
    let (show, light, size) = (flag("show")?, flag("light")?, number(p, "size")?);
    let place = match p.get("place") {
        None => None,
        Some(v) => Some(
            serde_json::from_value::<Place>(v.clone())
                .map_err(|_| EngineError::Params("`place` is topLeft, top, topRight, bottomLeft, bottom or bottomRight".to_string()))?,
        ),
    };
    let mut look = s.timeline.keys;
    look.show = show.unwrap_or(look.show);
    look.light = light.unwrap_or(look.light);
    look.place = place.unwrap_or(look.place);
    look.size = size.unwrap_or(look.size);
    s.timeline.keys = look.checked();
    Ok(json!({}))
}

/// A layer: its id, or `"new"` (or nothing) for a new one.
fn layer_or_new(p: &Value, key: &str) -> Result<Option<u32>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s == "new" => Ok(None),
        Some(_) => id(p, key).map(Some),
    }
}

fn number(p: &Value, key: &str) -> Result<Option<f32>> {
    match p.get(key) {
        None => Ok(None),
        Some(v) => {
            v.as_f64().filter(|f| f.is_finite()).map(|f| Some(f as f32)).ok_or_else(|| EngineError::Params(format!("`{key}` must be a number")))
        }
    }
}

fn text(p: &Value, key: &str) -> Result<Option<String>> {
    match p.get(key) {
        None => Ok(None),
        Some(v) => v.as_str().map(|s| Some(s.to_string())).ok_or_else(|| EngineError::Params(format!("`{key}` must be text"))),
    }
}

fn add_media(s: &mut VideoSession, p: &Value) -> Result<Value> {
    use screengrabcraft_media::layers::{MediaItem, MediaKind};
    let kind = serde_json::from_value::<MediaKind>(p.get("kind").cloned().unwrap_or(Value::Null))
        .map_err(|_| EngineError::Params("`kind` is picture, sound or video".to_string()))?;
    let name = text(p, "name")?.unwrap_or_default();
    let path = text(p, "path")?.unwrap_or_default();
    let duration = if p.get("duration").is_some() { int(p, "duration")? } else { 0 };
    let size = |k: &str| -> Result<u32> { if p.get(k).is_some() { Ok(id(p, k)?.min(100_000)) } else { Ok(0) } };
    let m = MediaItem { id: 0, name, path, kind, duration, width: size("width")?, height: size("height")? };
    let id = s.timeline.add_media(m).map_err(edit_err)?;
    Ok(json!({"id": id}))
}

fn remove_media(s: &mut VideoSession, p: &Value) -> Result<Value> {
    s.timeline.remove_media(id(p, "id")?).map_err(edit_err)?;
    Ok(json!({}))
}

fn place(s: &mut VideoSession, p: &Value) -> Result<Value> {
    use screengrabcraft_media::layers::{Item, What, template};
    let mut item = match (p.get("template").and_then(Value::as_str), p.get("media")) {
        (Some(key), _) => template(key).map(|t| t.item).ok_or_else(|| EngineError::Params(format!("there's no `{key}` in the library")))?,
        (None, Some(_)) => Item { what: What::Media { id: id(p, "media")? }, ..Item::default() },
        (None, None) => return Err(EngineError::Params("say what to place: `template` or `media`".to_string())),
    };
    if let What::Text { title, subtitle, .. } = &mut item.what {
        if let Some(t) = text(p, "title")? {
            *title = t;
        }
        if let Some(t) = text(p, "subtitle")? {
            *subtitle = t;
        }
    }
    if p.get("len").is_some() {
        item.len = int(p, "len")?;
    }
    let (layer, item) = s.timeline.place(item, layer_or_new(p, "layer")?, int(p, "at")?).map_err(edit_err)?;
    Ok(json!({"layer": layer, "item": item}))
}

fn move_item(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let (layer, item) = s.timeline.move_item(id(p, "layer")?, id(p, "item")?, layer_or_new(p, "toLayer")?, int(p, "at")?).map_err(edit_err)?;
    Ok(json!({"layer": layer, "item": item}))
}

fn trim_item(s: &mut VideoSession, p: &Value) -> Result<Value> {
    s.timeline.trim_item(id(p, "layer")?, id(p, "item")?, int(p, "start")?, int(p, "end")?).map_err(edit_err)?;
    Ok(json!({}))
}

fn delete_item(s: &mut VideoSession, p: &Value) -> Result<Value> {
    s.timeline.delete_item(id(p, "layer")?, id(p, "item")?).map_err(edit_err)?;
    Ok(json!({}))
}

fn set_item(s: &mut VideoSession, p: &Value) -> Result<Value> {
    use screengrabcraft_media::layers::{AnimKind, What};
    let (title, subtitle, color) = (text(p, "title")?, text(p, "subtitle")?, text(p, "color")?);
    let (x, y, w, opacity) = (number(p, "x")?, number(p, "y")?, number(p, "w")?, number(p, "opacity")?);
    let anim = |k: &str| -> Result<Option<AnimKind>> {
        match p.get(k) {
            None => Ok(None),
            Some(v) => serde_json::from_value::<AnimKind>(v.clone())
                .map(Some)
                .map_err(|_| EngineError::Params(format!("`{k}` is none, fade, slideLeft, slideRight, slideUp or pop"))),
        }
    };
    let (intro, outro) = (anim("intro")?, anim("outro")?);
    let (intro_len, outro_len) = (
        if p.get("introLen").is_some() { Some(int(p, "introLen")?) } else { None },
        if p.get("outroLen").is_some() { Some(int(p, "outroLen")?) } else { None },
    );
    s.timeline
        .set_item(id(p, "layer")?, id(p, "item")?, |i| {
            if let What::Text { title: t, subtitle: st, color: c, .. } = &mut i.what {
                if let Some(v) = title {
                    *t = v;
                }
                if let Some(v) = subtitle {
                    *st = v;
                }
                if let Some(v) = color {
                    *c = v;
                }
            }
            for (slot, v) in [(&mut i.place.x, x), (&mut i.place.y, y), (&mut i.place.w, w), (&mut i.place.opacity, opacity)] {
                if let Some(v) = v {
                    *slot = v;
                }
            }
            if let Some(k) = intro {
                i.intro.kind = k;
            }
            if let Some(k) = outro {
                i.outro.kind = k;
            }
            if let Some(l) = intro_len {
                i.intro.len = l.min(10_000_000);
            }
            if let Some(l) = outro_len {
                i.outro.len = l.min(10_000_000);
            }
        })
        .map_err(edit_err)?;
    Ok(json!({}))
}

/// A point of a sound track's volume line: added, changed or moved.
fn volume_point(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let point = if p.get("point").is_some_and(|v| !v.is_null()) { Some(index(p, "point")?) } else { None };
    let k = s.timeline.set_volume_point(index(p, "track")?, point, int(p, "at")?, number(p, "level")?).map_err(edit_err)?;
    Ok(json!({"point": k}))
}

fn remove_volume_point(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let all = p.get("all").and_then(Value::as_bool).unwrap_or(false);
    let point = if all { None } else { Some(index(p, "point")?) };
    s.timeline.remove_volume_point(index(p, "track")?, point).map_err(edit_err)?;
    Ok(json!({}))
}

/// The rest of the sound lowered while the voice (the microphone) is heard.
fn ducking(s: &mut VideoSession, p: &Value) -> Result<Value> {
    if !p.is_object() {
        return Err(EngineError::Params("say `on` and or `level`".to_string()));
    }
    let on = match p.get("on") {
        None => None,
        Some(v) => Some(v.as_bool().ok_or_else(|| EngineError::Params("`on` must be true or false".to_string()))?),
    };
    let d = &mut s.timeline.ducking;
    d.on = on.unwrap_or(d.on);
    if let Some(l) = number(p, "level")? {
        d.level = l.clamp(0.0, 1.0);
    }
    Ok(json!({"on": d.on, "level": d.level}))
}

/// How loud speech and music are made when levels are evened out (dB of the full scale).
const LEVEL_TARGET: f32 = -20.0;

/// Each sound track (or one) made as loud as the others: its volume set so what it plays comes
/// to `target` dB (as near as the volume goes).
fn levels(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let measure = s.measure.as_ref().ok_or_else(|| EngineError::Failed("levels can't be measured here".to_string()))?;
    let target = number(p, "target")?.unwrap_or(LEVEL_TARGET).clamp(-40.0, -6.0);
    let which: Vec<usize> = if p.get("track").is_some_and(|t| !t.is_null()) {
        vec![index(p, "track")?]
    } else {
        s.timeline.tracks.iter().enumerate().filter(|(_, t)| !t.kind.is_video()).map(|(i, _)| i).collect()
    };
    let mut done = Vec::new();
    for i in which {
        let t = s.timeline.track(i).map_err(edit_err)?;
        if t.kind.is_video() {
            return Err(EngineError::Params("only a sound track has a level".to_string()));
        }
        // Nothing heard on it: left as it is.
        let Some(db) = measure(&s.timeline, i).filter(|d| d.is_finite()) else { continue };
        let volume = 10f32.powf((target - db) / 20.0).clamp(0.0, 2.0);
        done.push((i, volume));
    }
    let mut out = Vec::new();
    for (i, v) in done {
        s.timeline.set_volume(i, v).map_err(edit_err)?;
        out.push(json!({"track": i, "volume": v}));
    }
    Ok(json!({"tracks": out}))
}

/// The transition from the clip before into a clip, where they meet (or none).
fn transition(s: &mut VideoSession, p: &Value) -> Result<Value> {
    use screengrabcraft_media::timeline::Blend;
    let kind = match p.get("kind") {
        None => {
            return Err(EngineError::Params(
                "`kind` is missing: crossFade, dipBlack, wipeLeft, wipeRight, wipeUp, wipeDown, push, circle or none".to_string(),
            ));
        }
        Some(Value::String(k)) if k == "none" => None,
        Some(v) => Some(serde_json::from_value::<Blend>(v.clone()).map_err(|_| {
            EngineError::Params("`kind` is crossFade, dipBlack, wipeLeft, wipeRight, wipeUp, wipeDown, push, circle or none".to_string())
        })?),
    };
    let len = if p.get("len").is_some_and(|v| !v.is_null()) { Some(int(p, "len")?) } else { None };
    let (track, clip) = (index(p, "track")?, index(p, "clip")?);
    s.timeline.set_transition(track, clip, kind, len).map_err(edit_err)?;
    let x = s.timeline.tracks.get(track).and_then(|t| t.clips.get(clip)).and_then(|c| c.transition);
    Ok(json!({"kind": x.map(|x| x.kind), "len": x.map(|x| x.len)}))
}

/// A keyframe of a layer item at a moment: its place, size and opacity then.
fn keyframe(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let (x, y, w, opacity) = (number(p, "x")?, number(p, "y")?, number(p, "w")?, number(p, "opacity")?);
    let (layer, item) = (id(p, "layer")?, id(p, "item")?);
    let key = s
        .timeline
        .set_key(layer, item, int(p, "at")?, |place| {
            for (slot, v) in [(&mut place.x, x), (&mut place.y, y), (&mut place.w, w), (&mut place.opacity, opacity)] {
                if let Some(v) = v {
                    *slot = v;
                }
            }
        })
        .map_err(edit_err)?;
    let keys = s.timeline.item(layer, item).map_or(0, |i| i.keys.len());
    Ok(json!({"key": key, "keys": keys}))
}

fn remove_keyframe(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let all = p.get("all").and_then(Value::as_bool).unwrap_or(false);
    let key = if all { None } else { Some(index(p, "key")?) };
    s.timeline.remove_key(id(p, "layer")?, id(p, "item")?, key).map_err(edit_err)?;
    Ok(json!({}))
}

fn layer(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let hidden = match p.get("hidden") {
        None => None,
        Some(v) => Some(v.as_bool().ok_or_else(|| EngineError::Params("`hidden` must be true or false".to_string()))?),
    };
    let name = text(p, "name")?;
    s.timeline.set_layer(id(p, "layer")?, hidden, number(p, "volume")?, name.as_deref()).map_err(edit_err)?;
    Ok(json!({}))
}

fn rename_track(s: &mut VideoSession, p: &Value) -> Result<Value> {
    let name = text(p, "name")?.unwrap_or_default();
    s.timeline.rename_track(index(p, "track")?, &name).map_err(edit_err)?;
    Ok(json!({}))
}

fn move_track(s: &mut VideoSession, p: &Value) -> Result<Value> {
    s.timeline.move_track(index(p, "track")?, index(p, "to")?).map_err(edit_err)?;
    Ok(json!({}))
}

fn move_layer(s: &mut VideoSession, p: &Value) -> Result<Value> {
    s.timeline.move_layer(id(p, "layer")?, index(p, "to")?).map_err(edit_err)?;
    Ok(json!({}))
}

fn delete_layer(s: &mut VideoSession, p: &Value) -> Result<Value> {
    s.timeline.delete_layer(id(p, "layer")?).map_err(edit_err)?;
    Ok(json!({}))
}

/// What's taken away from the camera's picture: nothing, a colour (a green screen), or the
/// background (learnt by the background model, see `screengrabcraft_media::keyer`).
fn camera_key(s: &mut VideoSession, p: &Value) -> Result<Value> {
    use screengrabcraft_media::keyer::Remove;
    if !p.is_object() {
        return Err(EngineError::Params("what's taken away is given as an object".to_string()));
    }
    let remove = match p.get("remove") {
        None => None,
        Some(v) => Some(
            serde_json::from_value::<Remove>(v.clone()).map_err(|_| EngineError::Params("`remove` is nothing, colour or background".to_string()))?,
        ),
    };
    let colour = match text(p, "colour")? {
        None => None,
        Some(c) => {
            let h = c.trim().trim_start_matches('#');
            let byte = |i: usize| h.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok());
            match (h.len(), byte(0), byte(2), byte(4)) {
                (6, Some(r), Some(g), Some(b)) => Some([r, g, b]),
                _ => return Err(EngineError::Params("`colour` is #rrggbb".to_string())),
            }
        }
    };
    let tolerance = number(p, "tolerance")?;
    let c = &mut s.timeline.camera;
    c.remove = remove.unwrap_or(c.remove);
    c.key_colour = colour.unwrap_or(c.key_colour);
    c.key_tolerance = tolerance.map_or(c.key_tolerance, |t| t.clamp(0.0, 1.0));
    Ok(json!({}))
}

fn camera_at(s: &mut VideoSession, p: &Value) -> Result<Value> {
    if p.get("corner").is_some_and(|v| !v.is_null()) || p.get("x").is_none() {
        // Back in its corner.
        s.timeline.camera.at = None;
        return camera(s, p);
    }
    let (x, y) = (number(p, "x")?.unwrap_or(0.5), number(p, "y")?.unwrap_or(0.5));
    s.timeline.camera.at = Some([x.clamp(-0.5, 1.5), y.clamp(-0.5, 1.5)]);
    if let Some(size) = number(p, "size")? {
        s.timeline.camera.size = size.clamp(0.1, 0.5);
    }
    Ok(json!({}))
}

pub const VIDEO_COMMANDS: &[VideoCommand] = &[
    VideoCommand { id: "video.cut", label: "Cut", params: r#"{"from": µs, "to": µs}: takes the stretch out of every track"#, run: cut },
    VideoCommand {
        id: "video.lift",
        label: "Remove",
        params: r#"{"from": µs, "to": µs, "track"?: n}: takes the stretch out of one track, or all, leaving a gap (nothing moves)"#,
        run: lift,
    },
    VideoCommand { id: "video.split", label: "Split", params: r#"{"at": µs, "track"?: n}: one track, or all"#, run: split },
    VideoCommand { id: "video.deleteClip", label: "Delete Clip", params: r#"{"track": n, "clip": n}: leaves a gap"#, run: delete_clip },
    VideoCommand { id: "video.trimStart", label: "Trim", params: r#"{"track": n, "clip": n, "to": µs}"#, run: trim_start },
    VideoCommand { id: "video.trimEnd", label: "Trim", params: r#"{"track": n, "clip": n, "to": µs}"#, run: trim_end },
    VideoCommand { id: "video.moveClip", label: "Move Clip", params: r#"{"track": n, "clip": n, "to": µs}"#, run: move_clip },
    VideoCommand {
        id: "video.speed",
        label: "Speed",
        params: r#"{"from": µs, "to": µs, "speed": 0.1..10, "track"?: n}: plays the stretch faster or slower (2 twice as fast, 0.5 half, 1 as recorded) on every track, or one; what follows moves up to follow on, voices keep their pitch. Says where the stretch now ends"#,
        run: speed,
    },
    VideoCommand {
        id: "video.holdFrame",
        label: "Hold Frame",
        params: r#"{"at": µs, "len": µs}: the picture at that moment held for `len` (up to 10 minutes): every track waits, what follows moves along, the sound is silent meanwhile. Says the stretch held"#,
        run: hold_frame,
    },
    VideoCommand {
        id: "video.holdLength",
        label: "Hold Length",
        params: r#"{"track": n, "clip": n, "len": µs}: a held frame (that clip) held this long instead, with the other tracks' hold there; what follows moves to match"#,
        run: hold_length,
    },
    VideoCommand {
        id: "video.fade",
        label: "Fade",
        params: r#"{"track": n, "clip": n, "in"?: µs, "out"?: µs}: how long a sound clip's sound takes to come up from silence at its start and go down to it at its end (0: at once)"#,
        run: fade,
    },
    VideoCommand {
        id: "video.denoise",
        label: "Background Noise",
        params: r#"{"track": n, "amount": 0..1}: takes a sound track's steady background noise (a fan, a hum, hiss) out, learnt from the track itself; 0 takes nothing out"#,
        run: denoise,
    },
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
    VideoCommand {
        id: "video.pointer",
        label: "Pointer",
        params: r#"{"show"?: bool, "size"?: 0.5..4, "smooth"?: 0..1, "clicks"?: bool, "highlight"?: bool, "spotlight"?: bool, "magnify"?: bool, "clickSound"?: bool}: the mouse pointer on the video: rings at its clicks, a yellow disc under it, the picture dimmed but round it, a lens showing what's round it twice as large, a click heard at each click"#,
        run: pointer,
    },
    VideoCommand {
        id: "video.keys",
        label: "Keys Pressed",
        params: r#"{"show"?: bool, "place"?: "topLeft"|"top"|"topRight"|"bottomLeft"|"bottom"|"bottomRight", "size"?: 0.5..2.5, "light"?: bool}: the shortcuts pressed while recording, shown on the video as keys for a moment after each press"#,
        run: keys,
    },
    VideoCommand { id: "video.revert", label: "Revert to Recording", params: "{}: every track whole again", run: revert },
    VideoCommand {
        id: "video.captionText",
        label: "Edit Caption",
        params: r#"{"line": n, "text": text}: a line of the recording's captions (its place among them, in order) put right; no words: the line goes"#,
        run: caption_text,
    },
    VideoCommand {
        id: "video.captionSplit",
        label: "Split Caption",
        params: r#"{"line": n, "at"?: n}: the line cut in two between words, near its `at`th character (else its middle), its time shared as its words are"#,
        run: caption_split,
    },
    VideoCommand {
        id: "video.captionJoin",
        label: "Join Captions",
        params: r#"{"line": n}: the line and the same speaker's next line made one"#,
        run: caption_join,
    },
    VideoCommand {
        id: "video.captionTimes",
        label: "Move Caption",
        params: r#"{"line": n, "start"?: µs, "end"?: µs}: when the line starts and ends, in the recording's time"#,
        run: caption_times,
    },
    VideoCommand {
        id: "video.addMarker",
        label: "Add Marker",
        params: r#"{"at": µs, "name"?: text}: marks a moment on the timeline (a chapter of the video made); its place among the markers"#,
        run: add_marker,
    },
    VideoCommand {
        id: "video.removeMarker",
        label: "Delete Marker",
        params: r#"{"marker": n}: its place among the markers, in order"#,
        run: remove_marker,
    },
    VideoCommand { id: "video.renameMarker", label: "Rename Marker", params: r#"{"marker": n, "name": text}"#, run: rename_marker },
    VideoCommand {
        id: "video.moveMarker",
        label: "Move Marker",
        params: r#"{"marker": n, "at": µs}: not onto another marker; its new place among them"#,
        run: move_marker,
    },
    VideoCommand {
        id: "video.captionsHeard",
        label: "Captions as Heard",
        params: "{}: every line put right goes back to how it was heard",
        run: captions_heard,
    },
    VideoCommand {
        id: "video.addMedia",
        label: "Import Media",
        params: r#"{"kind": "picture"|"sound"|"video", "name": text, "path": text, "duration"?: µs, "width"?: px, "height"?: px}: into the media bin; its id"#,
        run: add_media,
    },
    VideoCommand { id: "video.removeMedia", label: "Remove Media", params: r#"{"id": n}: out of the bin, with its items"#, run: remove_media },
    VideoCommand {
        id: "video.place",
        label: "Add to the Timeline",
        params: r#"{"template": key | "media": id, "at": µs, "layer"?: id|"new", "title"?: text, "subtitle"?: text, "len"?: µs}: where it fits, never over another"#,
        run: place,
    },
    VideoCommand {
        id: "video.moveItem",
        label: "Move",
        params: r#"{"layer": id, "item": id, "toLayer"?: id|"new", "at": µs}: the nearest place it fits"#,
        run: move_item,
    },
    VideoCommand { id: "video.trimItem", label: "Trim", params: r#"{"layer": id, "item": id, "start": µs, "end": µs}"#, run: trim_item },
    VideoCommand { id: "video.deleteItem", label: "Delete", params: r#"{"layer": id, "item": id}"#, run: delete_item },
    VideoCommand {
        id: "video.setItem",
        label: "Change",
        params: r##"{"layer": id, "item": id, "title"?, "subtitle"?, "color"?: "#rrggbb", "x"?, "y"?, "w"?, "opacity"?: 0..1, "intro"?, "outro"?: "none"|"fade"|"slideLeft"|"slideRight"|"slideUp"|"pop", "introLen"?, "outroLen"?: µs}"##,
        run: set_item,
    },
    VideoCommand {
        id: "video.volumePoint",
        label: "Volume Point",
        params: r#"{"track": n, "at": µs, "level"?: 0..2, "point"?: n}: a point of a sound track's volume line at that moment (on one of its clips): how loud it is there (else what the line has there). With `point`, that point moved there. The line runs straight from point to point and stays with the sound through edits; its place among the points"#,
        run: volume_point,
    },
    VideoCommand {
        id: "video.removeVolumePoint",
        label: "Remove Volume Point",
        params: r#"{"track": n, "point": n} or {"track": n, "all": true}: none left, it's as loud all along"#,
        run: remove_volume_point,
    },
    VideoCommand {
        id: "video.ducking",
        label: "Lower Under the Voice",
        params: r#"{"on"?: bool, "level"?: 0..1}: the rest of the sound (the computer's, music and media) lowered to `level` while the microphone's voice is heard, coming back up after"#,
        run: ducking,
    },
    VideoCommand {
        id: "video.levels",
        label: "Even Out Levels",
        params: r#"{"track"?: n, "target"?: -40..-6 dB}: each sound track (or one) given the volume that makes what it plays as loud as the target (-20 dB of the full scale; as near as the volume goes); says each track's volume"#,
        run: levels,
    },
    VideoCommand {
        id: "video.transition",
        label: "Transition",
        params: r#"{"track": n, "clip": n, "kind": "crossFade"|"dipBlack"|"wipeLeft"|"wipeRight"|"wipeUp"|"wipeDown"|"push"|"circle"|"none", "len"?: µs}: how the clip before turns into this one where they meet (0.1 to 5 s, across the cut; on a sound track a cross-fade)"#,
        run: transition,
    },
    VideoCommand {
        id: "video.keyframe",
        label: "Keyframe",
        params: r#"{"layer": id, "item": id, "at": µs, "x"?, "y"?, "w"?, "opacity"?: 0..1}: a keyframe of the item at that moment of the timeline (in it): where it is, how large and how see-through then (what isn't given: as it is there now); one there already is changed. It moves eased from one keyframe to the next"#,
        run: keyframe,
    },
    VideoCommand {
        id: "video.removeKeyframe",
        label: "Remove Keyframe",
        params: r#"{"layer": id, "item": id, "key": n} or {"layer", "item", "all": true}: with none left it stays where the last had it"#,
        run: remove_keyframe,
    },
    VideoCommand { id: "video.layer", label: "Layer", params: r#"{"layer": id, "hidden"?: bool, "volume"?: 0..2, "name"?: text}"#, run: layer },
    VideoCommand { id: "video.deleteLayer", label: "Delete Layer", params: r#"{"layer": id}"#, run: delete_layer },
    VideoCommand {
        id: "video.renameTrack",
        label: "Rename Track",
        params: r#"{"track": n, "name": text}: its header's name and the transcript's for its words; empty: what it is again"#,
        run: rename_track,
    },
    VideoCommand {
        id: "video.moveTrack",
        label: "Move Track",
        params: r#"{"track": n, "to": n}: its place among the tracks, 0 the top"#,
        run: move_track,
    },
    VideoCommand {
        id: "video.moveLayer",
        label: "Move Layer",
        params: r#"{"layer": id, "to": n}: its place among the layers, 0 at the back"#,
        run: move_layer,
    },
    VideoCommand {
        id: "video.cameraKey",
        label: "Camera Background",
        params: r##"{"remove"?: "nothing"|"colour"|"background", "colour"?: "#rrggbb", "tolerance"?: 0..1}: what's taken away from the camera's picture so the screen shows round you: a colour (a green screen; how far from it still goes), or the background (learnt from a few of the camera's pictures by the background model, the camera kept still)"##,
        run: camera_key,
    },
    VideoCommand {
        id: "video.cameraAt",
        label: "Move Camera",
        params: r#"{"x": -0.5..1.5, "y": -0.5..1.5, "size"?: 0.1..0.5}: its middle, as a share of the picture (past 0 or 1 it is off it); {"corner": …} puts it back in a corner"#,
        run: camera_at,
    },
];

#[cfg(test)]
mod tests {
    use screengrabcraft_media::recording::{Header, RecordingWriter, StreamInfo, StreamKind};
    use screengrabcraft_model::{Color, Raster};

    use super::*;

    fn recording() -> Recording {
        let streams = vec![
            StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 16, height: 16, channels: 0 },
            StreamInfo { id: 2, kind: StreamKind::Microphone, name: String::new(), width: 0, height: 0, channels: 1 },
        ];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        for t in 0..50u64 {
            w.video_frame(0, t * 100_000, &Raster::filled(16, 16, Color::rgb(t as u8, 0, 0)).unwrap()).unwrap();
            w.audio(2, t * 4800, &vec![100; 4800]).unwrap();
        }
        Recording::open(Box::new(w.finish(5_000_000).unwrap())).unwrap()
    }

    fn session() -> VideoSession {
        VideoSession::new(&recording())
    }

    #[test]
    fn a_volume_line_lowering_under_the_voice_and_even_levels_are_edits_like_any() {
        let mut s = session();
        s.execute("video.volumePoint", &json!({"track": 1, "at": 1_000_000, "level": 0.25})).unwrap();
        let v = s.execute("video.volumePoint", &json!({"track": 1, "at": 3_000_000, "level": 1.5})).unwrap();
        assert_eq!(v["point"].as_u64(), Some(1));
        assert_eq!(s.undo_label(), Some("Volume Point"));
        assert!((s.timeline.tracks[1].level_at(2_000_000) - 0.875).abs() < 1e-6);
        // Dragged, it moves; taken away, it's gone; the screen has none.
        s.execute("video.volumePoint", &json!({"track": 1, "point": 0, "at": 500_000})).unwrap();
        assert_eq!(s.timeline.tracks[1].points[0].at, 500_000);
        assert!(s.execute("video.volumePoint", &json!({"track": 0, "at": 500_000})).is_err());
        s.execute("video.removeVolumePoint", &json!({"track": 1, "point": 1})).unwrap();
        assert_eq!(s.timeline.tracks[1].points.len(), 1);
        // Lowered under the voice.
        s.execute("video.ducking", &json!({"on": true, "level": 0.2})).unwrap();
        assert!(s.timeline.ducking.on && (s.timeline.ducking.level - 0.2).abs() < 1e-6);
        // Levels: not without a way to measure them; with one, the volume that reaches the target.
        assert!(s.execute("video.levels", &json!({})).is_err());
        s.set_measure(Box::new(|_, _| Some(-26.0)));
        let v = s.execute("video.levels", &json!({})).unwrap();
        assert_eq!(v["tracks"].as_array().map(Vec::len), Some(1));
        assert!((s.timeline.tracks[1].volume - 2.0).abs() < 0.01, "-26 dB to -20 is twice as loud");
        assert!(s.execute("video.levels", &json!({"track": 0})).is_err());
        for _ in 0..7 {
            s.execute("edit.undo", &json!({})).unwrap();
        }
        assert!(s.timeline.tracks[1].points.is_empty() && !s.timeline.ducking.on && s.timeline.tracks[1].volume == 1.0);
    }

    #[test]
    fn tracks_are_renamed_and_moved_and_stay_so_when_opened_again() {
        let mut s = session();
        s.execute("video.renameTrack", &json!({"track": 1, "name": "Bill"})).unwrap();
        s.execute("video.moveTrack", &json!({"track": 1, "to": 0})).unwrap();
        assert_eq!(s.timeline.tracks[0].kind, StreamKind::Microphone);
        assert_eq!(s.timeline.tracks[0].name.as_deref(), Some("Bill"));
        assert_eq!(s.undo_label(), Some("Move Track"));
        // Written with the recording and read again: as they were left.
        let saved: screengrabcraft_media::timeline::Timeline = serde_json::from_value(serde_json::to_value(&s.timeline).unwrap()).unwrap();
        let fresh = session();
        let rec_kinds = |t: &screengrabcraft_media::timeline::Timeline| t.tracks.iter().map(|t| t.kind).collect::<Vec<_>>();
        assert_eq!(rec_kinds(&fresh.timeline), vec![StreamKind::Screen, StreamKind::Microphone], "the recording's own order");
        let again = saved.checked(&recording());
        assert_eq!(rec_kinds(&again), vec![StreamKind::Microphone, StreamKind::Screen]);
        assert_eq!(again.tracks[0].name.as_deref(), Some("Bill"));
        // Undone, back as recorded.
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.timeline.tracks[1].name, None);
        assert!(s.execute("video.moveTrack", &json!({"track": 9, "to": 0})).is_err());
        assert!(s.execute("video.renameTrack", &json!({"track": 0, "name": 5})).is_err());
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
    fn removing_a_stretch_from_one_track_leaves_a_gap_and_is_undone() {
        let mut s = session();
        s.execute("video.lift", &json!({"from": 1_000_000, "to": 2_000_000, "track": 1})).unwrap();
        assert_eq!(s.timeline.length(), 5_000_000, "nothing moved up");
        assert_eq!(s.timeline.source_time(1, 1_500_000), None);
        assert_eq!(s.timeline.source_time(0, 1_500_000), Some(1_500_000), "the screen plays on");
        assert_eq!(s.undo_label(), Some("Remove"));
        s.execute("video.lift", &json!({"from": 3_000_000, "to": 4_000_000})).unwrap();
        assert_eq!(s.timeline.source_time(0, 3_500_000), None);
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.timeline.source_time(1, 1_500_000), Some(1_500_000));
        assert!(s.execute("video.lift", &json!({"from": 2, "to": 1})).is_err());
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
    fn a_stretch_sped_up_is_one_undo_step_and_says_where_it_ends() {
        let mut s = session();
        let r = s.execute("video.speed", &json!({"from": 1_000_000, "to": 3_000_000, "speed": 2})).unwrap();
        assert_eq!((r["to"].as_u64(), r["length"].as_u64(), r["speed"].as_f64()), (Some(2_000_000), Some(4_000_000), Some(2.0)));
        assert_eq!(s.undo_label(), Some("Speed"));
        assert_eq!(s.timeline.source_time(1, 1_500_000), Some(2_000_000), "the sound with the picture");
        // Slower than the slowest is the slowest; one track alone.
        let r = s.execute("video.speed", &json!({"from": 0, "to": 1_000_000, "speed": 0.01, "track": 0})).unwrap();
        assert_eq!(r["speed"].as_f64(), Some(0.1));
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.timeline.length(), 5_000_000);
        for bad in [json!({"from": 0, "to": 1}), json!({"from": 0, "to": 9, "speed": 0}), json!({"from": 0, "to": 9, "speed": "fast"})] {
            assert!(s.execute("video.speed", &bad).is_err(), "{bad}");
        }
        assert!(!s.edited());
    }

    #[test]
    fn a_frame_is_held_made_longer_and_undone_each_one_undo_step() {
        let mut s = session();
        let r = s.execute("video.holdFrame", &json!({"at": 2_000_000, "len": 1_500_000})).unwrap();
        assert_eq!((r["from"].as_u64(), r["to"].as_u64(), r["length"].as_u64()), (Some(2_000_000), Some(3_500_000), Some(6_500_000)));
        assert_eq!(s.undo_label(), Some("Hold Frame"));
        assert_eq!(s.timeline.source_time(0, 3_000_000), Some(2_000_000), "the picture held");
        assert_eq!(s.timeline.source_time(1, 3_000_000), None, "the sound silent");
        s.execute("video.holdLength", &json!({"track": 0, "clip": 1, "len": 3_000_000})).unwrap();
        assert_eq!(s.timeline.length(), 8_000_000);
        for (id, bad) in [
            ("video.holdFrame", json!({"at": 99_000_000, "len": 1_000_000})),
            ("video.holdFrame", json!({"at": 0})),
            ("video.holdLength", json!({"track": 0, "clip": 0, "len": 1_000_000})),
            ("video.holdLength", json!({"track": 0, "clip": 9, "len": 1_000_000})),
        ] {
            assert!(s.execute(id, &bad).is_err(), "{id} {bad}");
        }
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(!s.edited());
    }

    #[test]
    fn a_sound_clip_fades_and_its_tracks_noise_is_taken_out_each_one_undo_step() {
        let mut s = session();
        let r = s.execute("video.fade", &json!({"track": 1, "clip": 0, "in": 1_500_000, "out": 99_000_000})).unwrap();
        assert_eq!((r["in"].as_u64(), r["out"].as_u64()), (Some(1_500_000), Some(3_500_000)), "the fade out given has its way, up to the clip");
        assert_eq!(s.undo_label(), Some("Fade"));
        s.execute("video.denoise", &json!({"track": 1, "amount": 0.6})).unwrap();
        assert_eq!(s.timeline.tracks[1].denoise, 0.6);
        assert_eq!(s.undo_label(), Some("Background Noise"));
        for bad in [
            json!({"track": 0, "clip": 0, "in": 5}),
            json!({"track": 1, "clip": 0}),
            json!({"track": 1, "clip": 7, "in": 5}),
            json!({"track": 0, "amount": 0.5}),
            json!({"track": 1, "amount": "lots"}),
        ] {
            let id = if bad.get("amount").is_some() { "video.denoise" } else { "video.fade" };
            assert!(s.execute(id, &bad).is_err(), "{id} {bad}");
        }
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(!s.edited());
    }

    #[test]
    fn captions_are_put_right_split_joined_and_moved_then_go_back_to_as_heard() {
        use screengrabcraft_media::captions::{Captions, Cue, Speaker};
        let streams = vec![StreamInfo { id: 2, kind: StreamKind::Microphone, name: String::new(), width: 0, height: 0, channels: 1 }];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        for t in 0..50u64 {
            w.audio(2, t * 4800, &vec![100; 4800]).unwrap();
        }
        let mut bytes = w.finish(5_000_000).unwrap();
        let heard = Captions {
            cues: vec![
                Cue { start: 500_000, end: 2_000_000, speaker: Speaker::You, text: "Hello their friends".into() },
                Cue { start: 3_000_000, end: 4_000_000, speaker: Speaker::You, text: "Bye".into() },
            ],
            model: String::new(),
        };
        bytes.extend(screengrabcraft_media::recording::captions_bytes(&heard).unwrap());
        let mut s = VideoSession::new(&Recording::open(Box::new(bytes)).unwrap());
        s.execute("video.captionText", &json!({"line": 0, "text": "Hello there friends"})).unwrap();
        assert_eq!(s.undo_label(), Some("Edit Caption"));
        s.execute("video.captionSplit", &json!({"line": 0})).unwrap();
        s.execute("video.captionJoin", &json!({"line": 1})).unwrap();
        s.execute("video.captionTimes", &json!({"line": 0, "start": 0, "end": 99_000_000})).unwrap();
        let words: Vec<(String, u64, u64)> = s.timeline.corrected.as_ref().unwrap().iter().map(|c| (c.text.clone(), c.start, c.end)).collect();
        assert_eq!(words[0], ("Hello there".to_string(), 0, 5_000_000), "moved, up to the recording's end");
        assert_eq!(words[1].0, "friends Bye");
        // Revert to Recording keeps them; Captions as Heard takes them back.
        s.execute("video.revert", &json!({})).unwrap();
        assert!(s.timeline.corrected.is_some());
        s.execute("video.captionsHeard", &json!({})).unwrap();
        assert!(s.timeline.corrected.is_none());
        for bad in [json!({"line": 9, "text": "x"}), json!({"line": 0}), json!({"line": "first", "text": "x"})] {
            assert!(s.execute("video.captionText", &bad).is_err(), "{bad}");
        }
        assert!(s.execute("video.captionTimes", &json!({"line": 0})).is_err());
        assert!(s.timeline.corrected.is_none(), "a command that fails changes nothing");
        // Captions that came later are what's put right from then on.
        s.set_heard(None);
        assert!(s.execute("video.captionJoin", &json!({"line": 0})).is_err());
    }

    #[test]
    fn markers_are_added_renamed_moved_and_deleted_each_one_undo_step() {
        let mut s = session();
        let r = s.execute("video.addMarker", &json!({"at": 3_000_000, "name": "Demo"})).unwrap();
        assert_eq!(r["marker"].as_u64(), Some(0));
        assert_eq!(s.undo_label(), Some("Add Marker"));
        s.execute("video.addMarker", &json!({"at": 1_000_000})).unwrap();
        s.execute("video.renameMarker", &json!({"marker": 0, "name": "Intro"})).unwrap();
        let r = s.execute("video.moveMarker", &json!({"marker": 0, "at": 4_000_000})).unwrap();
        assert_eq!(r["marker"].as_u64(), Some(1));
        assert_eq!(s.timeline.chapters(), vec![(0, "Start".to_string()), (3_000_000, "Demo".to_string()), (4_000_000, "Intro".to_string())]);
        s.execute("video.removeMarker", &json!({"marker": 0})).unwrap();
        assert_eq!(s.timeline.markers.len(), 1);
        for (id, bad) in [
            ("video.addMarker", json!({"at": 4_000_000})),
            ("video.addMarker", json!({"name": "x"})),
            ("video.renameMarker", json!({"marker": 0})),
            ("video.moveMarker", json!({"marker": 3, "at": 0})),
            ("video.removeMarker", json!({"marker": "first"})),
        ] {
            assert!(s.execute(id, &bad).is_err(), "{id} {bad}");
        }
        for _ in 0..5 {
            s.execute("edit.undo", &json!({})).unwrap();
        }
        assert!(s.timeline.markers.is_empty());
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
    fn the_cameras_colour_or_background_is_taken_away_and_undone() {
        use screengrabcraft_media::keyer::Remove;
        let mut s = session();
        s.execute("video.cameraKey", &json!({"remove": "colour", "colour": "#10F020", "tolerance": 7})).unwrap();
        let c = s.timeline.camera;
        assert_eq!((c.remove, c.key_colour, c.key_tolerance), (Remove::Colour, [0x10, 0xF0, 0x20], 1.0));
        assert_eq!(s.undo_label(), Some("Camera Background"));
        s.execute("video.cameraKey", &json!({"remove": "background"})).unwrap();
        assert_eq!(s.timeline.camera.remove, Remove::Background);
        for bad in [json!({"remove": "everything"}), json!({"colour": "green"}), json!({"colour": "#12345"}), json!({"tolerance": "lots"}), json!(1)]
        {
            assert!(s.execute("video.cameraKey", &bad).is_err(), "{bad}");
        }
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(!s.edited());
    }

    #[test]
    fn the_keys_look_is_set_kept_in_range_and_undone() {
        use screengrabcraft_media::keys::Place;
        let mut s = session();
        assert!(!s.timeline.keys.show, "off until asked for");
        s.execute("video.keys", &json!({"show": true, "place": "topRight", "size": 9, "light": true})).unwrap();
        let k = s.timeline.keys;
        assert_eq!((k.show, k.place, k.size, k.light), (true, Place::TopRight, 2.5, true));
        assert_eq!(s.undo_label(), Some("Keys Pressed"));
        for bad in [json!({"place": "middle"}), json!({"show": "yes"}), json!({"size": "big"}), json!(7)] {
            assert!(s.execute("video.keys", &bad).is_err(), "{bad}");
        }
        assert_eq!(s.timeline.keys, k, "a command that fails changes nothing");
        s.execute("edit.undo", &json!({})).unwrap();
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
            json!({"layer": 1e30, "item": -1, "at": -3, "template": 5, "media": "x", "kind": "movie", "x": "far", "toLayer": [1], "start": "now"}),
            json!({"layer": 999, "item": 999, "at": 0, "template": "no-such", "id": 999, "kind": "sound", "name": 5}),
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

#[cfg(test)]
mod layer_tests {
    use screengrabcraft_media::recording::{Header, RecordingWriter, StreamInfo, StreamKind};
    use screengrabcraft_model::{Color, Raster};

    use super::*;

    fn session() -> VideoSession {
        let streams = vec![StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 16, height: 16, channels: 0 }];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        for t in 0..50u64 {
            w.video_frame(0, t * 100_000, &Raster::filled(16, 16, Color::rgb(t as u8, 0, 0)).unwrap()).unwrap();
        }
        VideoSession::new(&Recording::open(Box::new(w.finish(5_000_000).unwrap())).unwrap())
    }

    #[test]
    fn a_lower_third_and_a_song_go_on_layers_of_their_own_and_undo() {
        let mut s = session();
        let placed =
            s.execute("video.place", &json!({"template": "lower-bar", "at": 1_000_000, "title": "Ann Lee", "subtitle": "Engineer"})).unwrap();
        let (layer, item) = (placed["layer"].as_u64().unwrap() as u32, placed["item"].as_u64().unwrap() as u32);
        assert_eq!(s.undo_label(), Some("Add to the Timeline"));
        match &s.timeline.item(layer, item).unwrap().what {
            screengrabcraft_media::layers::What::Text { title, subtitle, .. } => {
                assert_eq!((title.as_str(), subtitle.as_str()), ("Ann Lee", "Engineer"))
            }
            other => panic!("{other:?}"),
        }
        let song = s.execute("video.addMedia", &json!({"kind": "sound", "name": "Song.mp3", "path": "C:/Song.mp3", "duration": 2_000_000})).unwrap();
        let placed = s.execute("video.place", &json!({"media": song["id"], "at": 0, "layer": layer})).unwrap();
        assert_ne!(placed["layer"].as_u64().unwrap() as u32, layer, "sound on a sound layer, not the lower third's");
        s.execute("video.setItem", &json!({"layer": layer, "item": item, "x": 0.8, "intro": "pop", "title": "Bo"})).unwrap();
        assert_eq!(s.timeline.item(layer, item).unwrap().place.x, 0.8);
        s.execute("video.cameraAt", &json!({"x": 0.3, "y": 0.4})).unwrap();
        assert_eq!(s.timeline.camera.at, Some([0.3, 0.4]));
        s.execute("video.cameraAt", &json!({"corner": "topLeft"})).unwrap();
        assert_eq!(s.timeline.camera.at, None);
        for _ in 0..6 {
            s.execute("edit.undo", &json!({})).unwrap();
        }
        assert!(s.timeline.layers.is_empty() && s.timeline.media.is_empty());
    }

    #[test]
    fn clips_turn_into_each_other_where_they_meet_and_items_move_by_keyframes() {
        let streams = vec![
            StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 16, height: 16, channels: 0 },
            StreamInfo { id: 2, kind: StreamKind::Microphone, name: String::new(), width: 0, height: 0, channels: 1 },
        ];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        for t in 0..50u64 {
            w.video_frame(0, t * 100_000, &Raster::filled(16, 16, Color::rgb(t as u8, 0, 0)).unwrap()).unwrap();
            w.audio(2, t * 4800, &vec![100; 4800]).unwrap();
        }
        let mut s = VideoSession::new(&Recording::open(Box::new(w.finish(5_000_000).unwrap())).unwrap());
        s.execute("video.split", &json!({"at": 2_500_000})).unwrap();
        let v = s.execute("video.transition", &json!({"track": 0, "clip": 1, "kind": "wipeLeft", "len": 800_000})).unwrap();
        assert_eq!((v["kind"].as_str(), v["len"].as_u64()), (Some("wipeLeft"), Some(800_000)));
        assert_eq!(s.undo_label(), Some("Transition"));
        // The microphone only cross-fades; the first clip has nothing before it; a made-up kind is refused.
        let v = s.execute("video.transition", &json!({"track": 1, "clip": 1, "kind": "circle"})).unwrap();
        assert_eq!(v["kind"].as_str(), Some("crossFade"));
        assert!(s.execute("video.transition", &json!({"track": 0, "clip": 0, "kind": "push"})).is_err());
        assert!(s.execute("video.transition", &json!({"track": 0, "clip": 1, "kind": "spin"})).is_err());
        s.execute("video.transition", &json!({"track": 0, "clip": 1, "kind": "none"})).unwrap();
        assert_eq!(s.timeline.tracks[0].clips[1].transition, None);
        // A title moved from the left to the right by two keyframes.
        let placed = s.execute("video.place", &json!({"template": "title", "at": 0, "len": 4_000_000})).unwrap();
        let (layer, item) = (placed["layer"].as_u64().unwrap() as u32, placed["item"].as_u64().unwrap() as u32);
        s.execute("video.keyframe", &json!({"layer": layer, "item": item, "at": 0, "x": 0.2})).unwrap();
        let v = s.execute("video.keyframe", &json!({"layer": layer, "item": item, "at": 4_000_000, "x": 0.8})).unwrap();
        assert_eq!((v["key"].as_u64(), v["keys"].as_u64()), (Some(1), Some(2)));
        assert!((s.timeline.item(layer, item).unwrap().place_at(2_000_000).x - 0.5).abs() < 1e-4);
        assert!(s.execute("video.keyframe", &json!({"layer": layer, "item": item, "at": 9_000_000})).is_err());
        s.execute("video.removeKeyframe", &json!({"layer": layer, "item": item, "all": true})).unwrap();
        assert!(s.timeline.item(layer, item).unwrap().keys.is_empty());
        assert!(s.execute("video.removeKeyframe", &json!({"layer": layer, "item": item, "key": 0})).is_err());
    }
}
