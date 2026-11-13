//! The video editor: a Video Production recording opened in the editor. The picture at the
//! playhead fills the middle; below it the timeline has a row for each track (the screen, the
//! camera, the computer's sound, the microphone), each edited on its own.
//!
//! - Click the ruler (or anywhere empty) to move the playhead; drag across empty rows to choose
//!   a stretch, then Delete (or Cut) takes it out of every track.
//! - Click a clip to select it: Delete leaves a gap on that track only; drag its ends to trim
//!   it, or the clip itself to slide it. S splits at the playhead (the selected clip's track, or
//!   every track).
//! - Each sound track has its own mute and volume; the camera's corner, size and shape are on
//!   the right.
//! - Under the tracks, the captions: what you (the microphone) and the others (the computer's
//!   sound) said, made on this computer after the recording. Show puts them on the picture, In
//!   video burns them into the MP4; they follow the edits of the track they were heard on.
//!
//! Every edit is a `video.*` command of the engine's [`VideoSession`]; the edits are added to
//! the recording file a moment after each change, and Export Video makes the MP4 beside it
//! (with the captions beside it too, as `.srt` and `.txt`). Export Transcript saves what was
//! said, as edited, wherever the owner says: a transcript (`.txt`) or subtitles (`.srt`).

use std::collections::BTreeMap;
use std::sync::Arc;

use egui::{Color32, Key, KeyboardShortcut, Modifiers, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use screengrabcraft_engine::VideoSession;
use screengrabcraft_media::captions::{self, Captions, Cue, Speaker};
use screengrabcraft_media::compose::{self, Composer};
use screengrabcraft_media::recording::{self, Recording, StreamKind};
use screengrabcraft_media::timeline::{CameraShape, Corner, Track};
use serde_json::{Value, json};

mod export_window;
pub use export_window::{CaptionsIn, ExportChoice, ExportStep};

use crate::editor::Editor;
use crate::icons::{self, Icon};
use crate::services::{Filter, SoundOut, VideoFormat};
use crate::theme::Tokens;
use crate::widgets;

const HEADER_W: f32 = 176.0;
const RULER_H: f32 = 26.0;
const ROW_H: f32 = 48.0;
/// How near a clip's end the pointer must be to trim it.
const EDGE: f32 = 7.0;
/// Edits are written this long after the last change.
const SAVE_AFTER: f64 = 1.0;
/// The preview is made no larger than this (the export has its own size).
const PREVIEW_MAX: (u32, u32) = (1920, 1080);
/// The play button under the video, and the room kept under the picture for it.
const PLAY: f32 = 44.0;
const CONTROLS: f32 = PLAY + 20.0;
/// Sound waves are kept as peaks per 10 ms.
const WAVE_STEP: u64 = 480;
/// How often the host is asked whether captions being made have come (seconds).
const CAPTIONS_POLL: f64 = 2.0;
/// How often the host is asked otherwise, when the window is drawn anyway (seconds).
const CAPTIONS_GLANCE: f64 = 15.0;
/// Captions said to be coming that haven't after this long (seconds) can be asked for again.
const CAPTIONS_WAIT: f64 = 30.0 * 60.0;

/// What Export Transcript saves: the words as a transcript, or as subtitles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptFormat {
    /// A plain transcript (`.txt`): a paragraph for each turn, with when it started.
    Text,
    /// SubRip subtitles (`.srt`), for a video player or a video site.
    Subtitles,
}

impl TranscriptFormat {
    pub fn extension(self) -> &'static str {
        match self {
            TranscriptFormat::Text => "txt",
            TranscriptFormat::Subtitles => "srt",
        }
    }

    /// The one a file's name asks for, by its extension.
    pub fn of_path(path: &str) -> Option<TranscriptFormat> {
        let ext = std::path::Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "txt" => Some(TranscriptFormat::Text),
            "srt" => Some(TranscriptFormat::Subtitles),
            _ => None,
        }
    }

    /// The lines written out this way.
    pub fn write(self, cues: &[Cue]) -> String {
        match self {
            TranscriptFormat::Text => captions::to_text(cues),
            TranscriptFormat::Subtitles => captions::to_srt(cues),
        }
    }

    /// The name offered in the save dialog for a recording called `name`.
    fn file_name(self, name: &str) -> String {
        match self {
            TranscriptFormat::Text => format!("{name} transcript.txt"),
            TranscriptFormat::Subtitles => format!("{name} subtitles.srt"),
        }
    }

    /// The save dialog's file types, this one first.
    fn filters(self) -> [Filter; 2] {
        const TEXT: Filter = ("Transcript", &["txt"]);
        const SUBTITLES: Filter = ("Subtitles", &["srt"]);
        match self {
            TranscriptFormat::Text => [TEXT, SUBTITLES],
            TranscriptFormat::Subtitles => [SUBTITLES, TEXT],
        }
    }
}

/// What a drag on the timeline is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// Moving the playhead.
    Scrub,
    /// Choosing a stretch, from here.
    Range(u64),
    Trim {
        track: usize,
        clip: usize,
        start: bool,
    },
    /// Sliding a clip; the pointer is this far (µs) into it.
    Move {
        track: usize,
        clip: usize,
        grab: u64,
    },
}

struct Playing {
    /// When it started (egui's clock) and from where.
    since: f64,
    from: u64,
    sound: Option<Box<dyn SoundOut>>,
}

pub struct VideoView {
    pub rec: Arc<Recording>,
    pub session: VideoSession,
    /// The recording file, when it has one (edits are added to it).
    pub path: Option<String>,
    pub name: String,
    pub playhead: u64,
    playing: Option<Playing>,
    composer: Composer,
    texture: Option<egui::TextureHandle>,
    /// What the texture shows: (playhead, revision).
    shown: Option<(u64, u64)>,
    pub selected: Option<(usize, usize)>,
    /// A stretch chosen on the timeline.
    pub range: Option<(u64, u64)>,
    /// The timeline's zoom: pixels a second (0 until it's first fitted).
    pps: f32,
    scroll: f32,
    drag: Option<Drag>,
    /// The last revision written to the file, and when the edits last changed.
    saved: u64,
    seen: u64,
    changed_at: f64,
    /// Export Video's window: choosing, the video being made, or the video made.
    pub export: Option<ExportStep>,
    /// What Export Video was last set to (offered again).
    export_last: Option<(VideoFormat, CaptionsIn)>,
    /// Export Transcript was pressed before there were captions: they're being made, and the
    /// transcript is asked for when they come.
    pub transcript_after: Option<TranscriptFormat>,
    waves: BTreeMap<u8, Vec<(i16, i16)>>,
    /// The volume slider being dragged.
    volume_drag: bool,
    /// The recording's captions (in recording time), once they're made.
    pub captions: Option<Captions>,
    /// Bumped when they change.
    captions_rev: u64,
    /// The captions on the timeline as edited, and for which (edits, captions) they were placed.
    pub cues: Arc<Vec<Cue>>,
    cues_for: Option<(u64, u64)>,
    /// Whether the recording has a track with words to hear (the microphone, the computer's sound).
    pub has_sound: bool,
    /// Captions are being made (the service said so, or they were asked for here): since when.
    pub captions_coming: Option<f64>,
    /// The host's time of the captions file last seen, and when to ask again.
    captions_seen: u64,
    next_poll: f64,
}

fn kind_colour(kind: StreamKind) -> Color32 {
    match kind {
        StreamKind::Screen => Color32::from_rgb(0x6E, 0x5A, 0xD6),
        StreamKind::Camera => Color32::from_rgb(0x1F, 0x9D, 0x8B),
        StreamKind::SystemSound => Color32::from_rgb(0x2F, 0x80, 0xED),
        StreamKind::Microphone => Color32::from_rgb(0xE6, 0x7E, 0x22),
    }
}

/// `m:ss.t`
pub fn clock(us: u64) -> String {
    let tenths = us / 100_000;
    format!("{}:{:02}.{}", tenths / 600, (tenths / 10) % 60, tenths % 10)
}

/// Each sound track's peaks per 10 ms (for drawing its wave), the first two hours at most.
fn waves(rec: &Recording) -> BTreeMap<u8, Vec<(i16, i16)>> {
    let mut out = BTreeMap::new();
    for s in rec.header.streams.iter().filter(|s| !s.kind.is_video()) {
        let total = compose::to_samples(rec.stream_duration(s.id)).min(48_000 * 7200);
        let ch = usize::from(s.channels.clamp(1, 2));
        let mut peaks = Vec::with_capacity((total / WAVE_STEP) as usize + 1);
        let mut at = 0u64;
        while at < total {
            let n = (total - at).min(48_000 * 10) as usize;
            let block = rec.sound(s.id, at, n);
            for step in block.chunks(WAVE_STEP as usize * ch) {
                let lo = step.iter().copied().min().unwrap_or(0);
                let hi = step.iter().copied().max().unwrap_or(0);
                peaks.push((lo, hi));
            }
            at += n as u64;
        }
        out.insert(s.id, peaks);
    }
    out
}

impl VideoView {
    pub fn new(rec: Recording, path: Option<String>, name: String) -> VideoView {
        let session = VideoSession::new(&rec);
        let (w, h) = rec
            .header
            .streams
            .iter()
            .find(|s| s.kind == StreamKind::Screen)
            .or_else(|| rec.header.streams.iter().find(|s| s.kind == StreamKind::Camera))
            .map(|s| compose::fit(s.width, s.height, PREVIEW_MAX.0, PREVIEW_MAX.1))
            .unwrap_or((1280, 720));
        let waves = waves(&rec);
        let captions = rec.captions.clone().filter(|c| !c.is_empty());
        let has_sound = rec.header.streams.iter().any(|s| Speaker::of(s.kind).is_some());
        let rec = Arc::new(rec);
        let composer = Composer::new(rec.clone(), &session.timeline, w, h);
        VideoView {
            rec,
            session,
            path,
            name,
            playhead: 0,
            playing: None,
            composer,
            texture: None,
            shown: None,
            selected: None,
            range: None,
            pps: 0.0,
            scroll: 0.0,
            drag: None,
            saved: 0,
            seen: 0,
            changed_at: 0.0,
            export: None,
            export_last: None,
            transcript_after: None,
            waves,
            volume_drag: false,
            captions,
            captions_rev: 0,
            cues: Arc::new(Vec::new()),
            cues_for: None,
            has_sound,
            captions_coming: None,
            captions_seen: 0,
            next_poll: 0.0,
        }
    }

    /// Whether the recording's captions are on the picture (and there are some).
    fn showing_captions(&self) -> bool {
        self.session.timeline.captions.show && !self.cues.is_empty()
    }

    /// The captions placed on the timeline again after an edit, or when they came.
    fn place_captions(&mut self) {
        let key = (self.session.revision(), self.captions_rev);
        if self.cues_for == Some(key) {
            return;
        }
        self.cues_for = Some(key);
        self.cues = Arc::new(self.captions.as_ref().map(|c| c.on_timeline(&self.session.timeline)).unwrap_or_default());
        self.composer.set_captions(self.showing_captions().then(|| self.cues.clone()));
        self.shown = None;
    }

    /// The captions are being made (the service is on them); they're shown when they come.
    pub fn expect_captions(&mut self, now: f64) {
        if self.has_sound && self.path.is_some() {
            self.captions_coming = Some(now);
            self.next_poll = now;
        }
    }

    /// Asks the host for captions.
    pub fn make_captions(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let Some(path) = self.path.clone() else {
            ed.error(ctx, "Only a recording opened from a file can have captions made");
            return;
        };
        match ed.services.make_captions(&path) {
            Ok(()) => self.expect_captions(ctx.input(|i| i.time)),
            Err(e) => ed.error(ctx, e),
        }
    }

    /// Export Transcript: asks where to save what was said in the video as edited (words cut or
    /// muted are left out), as a transcript or as subtitles; in a browser it's downloaded.
    /// A recording with no captions yet has them made first (on this computer), and the
    /// transcript is asked for when they come.
    pub fn export_transcript(&mut self, ed: &mut Editor, ctx: &egui::Context, format: TranscriptFormat) {
        if self.cues.is_empty() {
            if self.captions.is_none() && self.has_sound && self.path.is_some() {
                if self.captions_coming.is_none() {
                    self.make_captions(ed, ctx);
                }
                if self.captions_coming.is_some() {
                    self.transcript_after = Some(format);
                    ed.toast(ctx, "Making the captions first, on this computer: you'll choose where the transcript goes when they're ready");
                }
                return;
            }
            ed.error(ctx, "There's nothing said to export: the words were all cut or muted");
            return;
        }
        let name = format.file_name(&self.name);
        if ed.services.has_paths() {
            ed.saving = None;
            ed.saving_project = false;
            ed.saving_transcript = Some(format);
            ed.services.request_save(&name, &format.filters());
        } else if let Err(e) = ed.services.download(&name, format.write(&self.cues).as_bytes()) {
            ed.error(ctx, e);
        }
    }

    /// Whether Export Transcript has something to save, or captions it can make first.
    fn can_export_transcript(&self) -> bool {
        !self.cues.is_empty() || (self.captions.is_none() && self.has_sound && self.path.is_some())
    }

    /// Whether to look for captions now: while there are none, or some are coming.
    fn waiting_for_captions(&self) -> bool {
        self.has_sound && self.path.is_some() && (self.captions.is_none() || self.captions_coming.is_some())
    }

    /// Asks the host whether captions have come.
    fn poll_captions(&mut self, ed: &mut Editor, ctx: &egui::Context, now: f64) {
        if !self.waiting_for_captions() {
            return;
        }
        if let Some(since) = self.captions_coming
            && now - since > CAPTIONS_WAIT
        {
            self.captions_coming = None;
        }
        if now >= self.next_poll {
            // Often while they're coming; now and then otherwise (made by the service meanwhile).
            self.next_poll = now + if self.captions_coming.is_some() { CAPTIONS_POLL } else { CAPTIONS_GLANCE };
            if let Some(path) = self.path.clone()
                && let Some((seen, caps)) = ed.services.poll_captions(&path, self.captions_seen)
            {
                self.captions_seen = seen;
                // The service has written its answer: some lines, or none heard.
                let came = self.captions_coming.take().is_some();
                match caps.filter(|c| !c.is_empty()) {
                    Some(c) => {
                        let n = c.cues.len();
                        self.captions = Some(c);
                        self.captions_rev += 1;
                        if came {
                            ed.toast(ctx, format!("Captions made: {n} lines"));
                        }
                    }
                    None if came => ed.toast(ctx, "No words were heard in this recording"),
                    None => {}
                }
            }
        }
        if self.captions_coming.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(CAPTIONS_POLL));
        }
    }

    fn length(&self) -> u64 {
        self.session.timeline.length()
    }

    fn fps(&self) -> u64 {
        u64::from(self.rec.header.fps.clamp(1, 60))
    }

    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    /// Runs a command; a failure is said in the editor's message.
    fn run(&mut self, ed: &mut Editor, ctx: &egui::Context, id: &str, params: Value) {
        if let Err(e) = self.session.execute(id, &params) {
            ed.error(ctx, e.to_string());
        }
        self.fix_selection();
    }

    fn fix_selection(&mut self) {
        if let Some((t, c)) = self.selected
            && self.session.timeline.tracks.get(t).is_none_or(|tr| c >= tr.clips.len())
        {
            self.selected = None;
        }
    }

    pub fn play(&mut self, ed: &Editor, now: f64) {
        if self.playhead >= self.length() {
            self.playhead = 0;
        }
        let sound = ed.services.play_sound(self.rec.clone(), self.session.timeline.clone(), self.playhead);
        self.playing = Some(Playing { since: now, from: self.playhead, sound });
    }

    pub fn pause(&mut self) {
        self.playing = None;
    }

    fn seek(&mut self, ed: &Editor, us: u64, now: f64) {
        self.playhead = us.min(self.length());
        if self.playing.is_some() {
            self.play(ed, now);
        }
    }

    /// Splits at the playhead: the selected clip's track, or every track.
    pub fn split(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let track = self.selected.map(|(t, _)| t);
        let p = match track {
            Some(t) => json!({"at": self.playhead, "track": t}),
            None => json!({"at": self.playhead}),
        };
        self.run(ed, ctx, "video.split", p);
    }

    /// Delete: the chosen stretch out of every track, else the selected clip.
    pub fn delete(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        if let Some((a, b)) = self.range.take() {
            self.run(ed, ctx, "video.cut", json!({"from": a, "to": b}));
            self.playhead = a.min(self.length());
        } else if let Some((t, c)) = self.selected.take() {
            self.run(ed, ctx, "video.deleteClip", json!({"track": t, "clip": c}));
        }
    }

    /// Writes the edits now if there are some to write.
    pub fn save_now(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        if self.session.revision() == self.saved {
            return;
        }
        let Some(path) = self.path.clone() else { return };
        match recording::edit_bytes(&self.session.timeline).map_err(|e| e.to_string()).and_then(|b| ed.services.append_file(&path, &b)) {
            Ok(()) => {
                self.saved = self.session.revision();
                self.session.dirty = false;
            }
            Err(e) => {
                // Not tried again until the next change.
                self.saved = self.session.revision();
                ed.error(ctx, format!("The edits could not be saved: {e}"));
            }
        }
    }

    /// Work done every frame: the playhead while playing, saving edits, the export, the captions.
    fn tick(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.poll_captions(ed, ctx, now);
        self.place_captions();
        // Export Transcript waiting for captions: asked for once they've come, let go if none do.
        if let Some(f) = self.transcript_after {
            if !self.cues.is_empty() {
                self.transcript_after = None;
                self.export_transcript(ed, ctx, f);
            } else if self.captions_coming.is_none() {
                self.transcript_after = None;
            }
        }
        if let Some(p) = &self.playing {
            let at = p.sound.as_ref().and_then(|s| s.position_us()).unwrap_or_else(|| p.from + ((now - p.since).max(0.0) * 1e6) as u64);
            if at >= self.length() {
                self.playhead = self.length();
                self.playing = None;
            } else {
                self.playhead = at;
                ctx.request_repaint();
            }
        }
        if self.session.revision() != self.seen {
            self.seen = self.session.revision();
            self.changed_at = now;
        }
        if self.session.revision() != self.saved && self.drag.is_none() && !self.volume_drag && now - self.changed_at >= SAVE_AFTER {
            self.save_now(ed, ctx);
        } else if self.session.revision() != self.saved {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
        self.poll_export(ed, ctx);
    }

    fn keys(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        // Export Video's windows take the keys (Enter, Esc) while one is up.
        if ed.dialog.is_some() || self.export.is_some() || ctx.egui_wants_keyboard_input() {
            return;
        }
        let now = ctx.input(|i| i.time);
        let cmd = |k: Key| KeyboardShortcut::new(Modifiers::COMMAND, k);
        let pressed = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
        let plain = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if pressed(KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)) || pressed(cmd(Key::Y)) {
            self.run(ed, ctx, "edit.redo", json!({}));
        } else if pressed(cmd(Key::Z)) {
            self.run(ed, ctx, "edit.undo", json!({}));
        }
        if pressed(cmd(Key::O)) {
            ed.services.request_open();
        }
        if pressed(cmd(Key::E)) {
            self.open_export(ed, ctx);
        }
        if pressed(cmd(Key::X)) && self.range.is_some() {
            self.delete(ed, ctx);
        }
        if plain(Key::Space) {
            if self.playing.is_some() { self.pause() } else { self.play(ed, now) }
        }
        if plain(Key::S) {
            self.split(ed, ctx);
        }
        if plain(Key::Delete) || plain(Key::Backspace) {
            self.delete(ed, ctx);
        }
        if plain(Key::Escape) {
            self.range = None;
            self.selected = None;
        }
        let frame = 1_000_000 / self.fps();
        if plain(Key::ArrowLeft) {
            self.seek(ed, self.playhead.saturating_sub(frame), now);
        }
        if plain(Key::ArrowRight) {
            self.seek(ed, self.playhead + frame, now);
        }
        if plain(Key::Home) {
            self.seek(ed, 0, now);
        }
        if plain(Key::End) {
            self.seek(ed, self.length(), now);
        }
    }

    fn menu_bar(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        egui::Panel::top("video_menu").frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(egui::Margin::symmetric(8, 3))).show(ui, |ui| {
            egui::MenuBar::new().config(crate::widgets::menu_config()).ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.add(egui::Button::new("Open…").shortcut_text("Ctrl+O")).clicked() {
                        ed.services.request_open();
                        ui.close();
                    }
                    if ui.button("New Capture").clicked() {
                        if let Err(e) = ed.services.new_capture(screengrabcraft_model::CaptureMode::Region) {
                            ed.error(&ctx, e);
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui.add_enabled(self.export.is_none(), egui::Button::new("Export Video…").shortcut_text("Ctrl+E")).clicked() {
                        self.open_export(ed, &ctx);
                        ui.close();
                    }
                    if let Some(p) = &self.path
                        && ui.button("Show in Folder").clicked()
                    {
                        ed.services.open_folder(Some(p));
                        ui.close();
                    }
                    if self.has_sound && self.captions.is_none() {
                        let can = self.path.is_some() && self.captions_coming.is_none();
                        if ui
                            .add_enabled(can, egui::Button::new("Make Captions"))
                            .on_hover_text("Write down what was said, on this computer, in the background")
                            .clicked()
                        {
                            self.make_captions(ed, &ctx);
                            ui.close();
                        }
                    }
                    if self.can_export_transcript() {
                        if ui.button("Export Transcript…").on_hover_text("What was said, as a text file (.txt)").clicked() {
                            self.export_transcript(ed, &ctx, TranscriptFormat::Text);
                            ui.close();
                        }
                        if ui.button("Export Subtitles…").on_hover_text("What was said, as subtitles (.srt) for a video player or site").clicked() {
                            self.export_transcript(ed, &ctx, TranscriptFormat::Subtitles);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui
                        .add_enabled(self.session.edited(), egui::Button::new("Revert to Recording"))
                        .on_hover_text("Every track whole, as recorded")
                        .clicked()
                    {
                        self.run(ed, &ctx, "video.revert", json!({}));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Close Window").clicked() {
                        ed.quit_requested = true;
                        ui.close();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    let undo = format!("Undo {}", self.session.undo_label().unwrap_or(""));
                    if ui.add_enabled(self.session.can_undo(), egui::Button::new(undo.trim()).shortcut_text("Ctrl+Z")).clicked() {
                        self.run(ed, &ctx, "edit.undo", json!({}));
                        ui.close();
                    }
                    let redo = format!("Redo {}", self.session.redo_label().unwrap_or(""));
                    if ui.add_enabled(self.session.can_redo(), egui::Button::new(redo.trim()).shortcut_text("Ctrl+Y")).clicked() {
                        self.run(ed, &ctx, "edit.redo", json!({}));
                        ui.close();
                    }
                    ui.separator();
                    if ui.add(egui::Button::new("Split at the Playhead").shortcut_text("S")).clicked() {
                        self.split(ed, &ctx);
                        ui.close();
                    }
                    let what = if self.range.is_some() { "Cut the Stretch" } else { "Delete the Clip" };
                    if ui.add_enabled(self.range.is_some() || self.selected.is_some(), egui::Button::new(what).shortcut_text("Del")).clicked() {
                        self.delete(ed, &ctx);
                        ui.close();
                    }
                });
                ui.menu_button("Play", |ui| {
                    let label = if self.playing.is_some() { "Pause" } else { "Play" };
                    if ui.add(egui::Button::new(label).shortcut_text("Space")).clicked() {
                        if self.playing.is_some() {
                            self.pause()
                        } else {
                            self.play(ed, now)
                        }
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Go to the Start").shortcut_text("Home")).clicked() {
                        self.seek(ed, 0, now);
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Go to the End").shortcut_text("End")).clicked() {
                        self.seek(ed, self.length(), now);
                        ui.close();
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.checkbox(&mut ed.settings.show_tray, "Recent Captures").changed() {
                        ed.services.save_settings(&ed.settings);
                    }
                    if ui.button("Fit the Timeline").clicked() {
                        self.pps = 0.0;
                        ui.close();
                    }
                    if self.captions.is_some() {
                        ui.separator();
                        let c = self.session.timeline.captions;
                        let mut show = c.show;
                        if ui.checkbox(&mut show, "Captions on the Picture").changed() {
                            self.run(ed, &ctx, "video.captions", json!({"show": show}));
                        }
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button(format!("About {}", screengrabcraft_model::EDITOR_NAME)).clicked() {
                        ed.dialog = Some(crate::dialogs::Dialog::About);
                        ui.close();
                    }
                });
            });
        });
    }

    fn tool_bar(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        egui::Panel::top("video_tools")
            .frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(egui::Margin::symmetric(8, 5)))
            .show_separator_line(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                    if icons::button(ui, Icon::Open, "Open (Ctrl+O)", false, true).clicked() {
                        ed.services.request_open();
                    }
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                    let (icon, tip) = if self.playing.is_some() { (Icon::Pause, "Pause (Space)") } else { (Icon::Play, "Play (Space)") };
                    if icons::button(ui, icon, tip, false, self.length() > 0).clicked() {
                        if self.playing.is_some() { self.pause() } else { self.play(ed, now) }
                    }
                    ui.label(RichText::new(format!("{} / {}", clock(self.playhead), clock(self.length()))).monospace().size(14.0).color(t.text));
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);
                    if widgets::button(ui, "Split").on_hover_text("Split at the playhead (S): the selected clip's track, or all of them").clicked() {
                        self.split(ed, &ctx);
                    }
                    let tip = if self.range.is_some() {
                        "Take the chosen stretch out of every track (Del)"
                    } else {
                        "Delete the selected clip, leaving a gap (Del)"
                    };
                    let any = self.range.is_some() || self.selected.is_some();
                    if ui
                        .add_enabled_ui(any, |ui| widgets::button(ui, if self.range.is_some() { "Cut" } else { "Delete" }))
                        .inner
                        .on_hover_text(tip)
                        .clicked()
                    {
                        self.delete(ed, &ctx);
                    }
                    ui.add_space(6.0);
                    let undo_tip = format!("Undo {} (Ctrl+Z)", self.session.undo_label().unwrap_or(""));
                    if icons::button(ui, Icon::Undo, &undo_tip, false, self.session.can_undo()).clicked() {
                        self.run(ed, &ctx, "edit.undo", json!({}));
                    }
                    let redo_tip = format!("Redo {} (Ctrl+Y)", self.session.redo_label().unwrap_or(""));
                    if icons::button(ui, Icon::Redo, &redo_tip, false, self.session.can_redo()).clicked() {
                        self.run(ed, &ctx, "edit.redo", json!({}));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                        let tip = "Choose where the video goes, what it's made as and whether its captions go with it (Ctrl+E)";
                        if widgets::primary_button(ui, "Export Video").on_hover_text(tip).clicked() {
                            self.open_export(ed, &ctx);
                        }
                        if self.has_sound {
                            let tip = if !self.cues.is_empty() {
                                "Save what was said as a transcript (.txt) or subtitles (.srt)"
                            } else {
                                "This recording has no captions yet: they're made first, on this computer, then you choose where the transcript goes"
                            };
                            let why_not = if self.transcript_after.is_some() {
                                "Making the captions first, on this computer…"
                            } else {
                                "Nothing said is left in the video as edited"
                            };
                            let can = self.can_export_transcript() && self.transcript_after.is_none();
                            let r = ui.add_enabled_ui(can, |ui| widgets::button(ui, "Export Transcript")).inner;
                            if r.on_hover_text(tip).on_disabled_hover_text(why_not).clicked() {
                                self.export_transcript(ed, &ctx, TranscriptFormat::Text);
                            }
                        }
                    });
                });
            });
    }

    fn status_bar(&self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        egui::Panel::bottom("video_status")
            .exact_size(24.0)
            .frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(egui::Margin::symmetric(10, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
                    let small = |s: String| RichText::new(s).size(12.0).color(t.muted);
                    let screen = self.rec.header.streams.iter().find(|s| s.kind == StreamKind::Screen);
                    if let Some(s) = screen {
                        ui.label(small(format!("{} × {} px, {} frames a second", s.width, s.height, self.rec.header.fps)));
                    }
                    ui.label(small(format!("{} long", clock(self.length()))));
                    if let Some((a, b)) = self.range {
                        ui.label(small(format!("Chosen: {} to {} — Delete takes it out", clock(a), clock(b))));
                    }
                    if !self.rec.finished {
                        ui.label(small("This recording stopped early: it's what was saved".to_string()));
                    }
                    if let Some(c) = &self.captions {
                        ui.label(small(format!("{} caption lines", c.cues.len())));
                    } else if self.captions_coming.is_some() {
                        ui.label(small("Making captions…".to_string()));
                    }
                    let saved = if self.session.revision() == self.saved { "Edits saved" } else { "Saving…" };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(small(saved.to_string()));
                    });
                });
            });
    }

    /// The camera's place in the picture.
    fn camera_panel(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let Some(track) = self.session.timeline.find(StreamKind::Camera) else { return };
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let cam = self.session.timeline.camera;
        egui::Panel::right("video_camera")
            .exact_size(236.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::same(14)))
            .show(ui, |ui| {
                ui.label(RichText::new("Camera").size(16.0).color(t.text));
                ui.add_space(8.0);
                ui.label(RichText::new("Corner").color(t.muted));
                egui::Grid::new("camera_corners").spacing([6.0, 6.0]).show(ui, |ui| {
                    for (i, (corner, label)) in [
                        (Corner::TopLeft, "Top left"),
                        (Corner::TopRight, "Top right"),
                        (Corner::BottomLeft, "Bottom left"),
                        (Corner::BottomRight, "Bottom right"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        if ui.add(egui::Button::new(label).selected(cam.corner == corner).min_size(vec2(96.0, 28.0))).clicked() {
                            self.run(ed, &ctx, "video.camera", json!({"corner": corner}));
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Shape").color(t.muted));
                ui.horizontal(|ui| {
                    for (shape, label) in [(CameraShape::Rectangle, "Square"), (CameraShape::Rounded, "Rounded"), (CameraShape::Circle, "Circle")] {
                        if ui.add(egui::Button::new(label).selected(cam.shape == shape)).clicked() {
                            self.run(ed, &ctx, "video.camera", json!({"shape": shape}));
                        }
                    }
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Size (share of the width)").color(t.muted));
                let mut size = cam.size * 100.0;
                let r = ui.add(egui::Slider::new(&mut size, 10.0..=50.0).suffix("%")).on_hover_text("Its width, as a share of the video's");
                if r.drag_started() {
                    self.session.begin("Camera");
                }
                if r.changed() {
                    let p = json!({"size": size / 100.0});
                    if r.dragged() {
                        let _ = self.session.drag("video.camera", &p);
                    } else {
                        self.run(ed, &ctx, "video.camera", p)
                    }
                }
                if r.drag_stopped() {
                    self.session.commit();
                }
                ui.add_space(12.0);
                let hidden = self.session.timeline.tracks.get(track).is_some_and(|tr| tr.clips.is_empty());
                if hidden {
                    ui.label(RichText::new("The camera's clips were all deleted.").color(t.muted));
                }
            });
    }

    /// The picture at the playhead, fitted to the space.
    fn preview(&mut self, ed: &Editor, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let key = (self.playhead, self.session.revision());
        if self.shown != Some(key) {
            let pic = self.composer.frame(&self.session.timeline, self.playhead);
            let img = egui::ColorImage::from_rgba_unmultiplied([pic.width() as usize, pic.height() as usize], pic.pixels());
            match &mut self.texture {
                Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
                None => self.texture = Some(ui.ctx().load_texture("video-frame", img, egui::TextureOptions::LINEAR)),
            }
            self.shown = Some(key);
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.canvas)).show(ui, |ui| {
            // The picture, and under it the play button with the time.
            let whole = ui.max_rect().shrink(16.0);
            let area = Rect::from_min_max(whole.min, pos2(whole.max.x, (whole.max.y - CONTROLS).max(whole.min.y)));
            let mut bottom = area.max.y;
            if let Some(tex) = &self.texture {
                let size = tex.size_vec2();
                let f = (area.width() / size.x).min(area.height() / size.y).clamp(0.01, 1.0);
                let r = Rect::from_center_size(area.center(), size * f);
                ui.painter().image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, t.border), StrokeKind::Outside);
                bottom = r.max.y;
            }
            let now = ui.ctx().input(|i| i.time);
            let playing = self.playing.is_some();
            let button = Rect::from_center_size(pos2(area.center().x, bottom + 10.0 + PLAY / 2.0), vec2(PLAY, PLAY));
            let tip = if playing { "Pause (Space)" } else { "Play (Space)" };
            let resp = ui.interact(button, ui.id().with("play-under-video"), egui::Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, self.length() > 0, tip));
            let fill = if resp.hovered() { t.accent.gamma_multiply(1.15) } else { t.accent };
            ui.painter().circle_filled(button.center(), PLAY / 2.0, fill);
            icons::paint(ui.painter(), button.shrink(PLAY * 0.27), if playing { Icon::Pause } else { Icon::Play }, Color32::WHITE);
            ui.painter().text(
                pos2(button.max.x + 14.0, button.center().y),
                egui::Align2::LEFT_CENTER,
                format!("{} / {}", clock(self.playhead), clock(self.length())),
                egui::FontId::monospace(14.0),
                t.text,
            );
            if resp.on_hover_text(tip).clicked() && self.length() > 0 {
                if playing {
                    self.pause();
                } else {
                    self.play(ed, now);
                }
            }
        });
    }

    fn header(&mut self, ed: &mut Editor, ui: &mut Ui, row: Rect, i: usize, track: &Track) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let name = self.rec.stream(track.stream).map(|s| s.kind.label()).unwrap_or("Track");
        ui.painter().rect_filled(Rect::from_min_size(row.min, vec2(4.0, row.height())), 0.0, kind_colour(track.kind));
        let painter = ui.painter_at(row);
        painter.text(row.min + vec2(14.0, 6.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(13.0), t.text);
        // Every track can be shown or hidden: a hidden picture isn't in the video, a hidden
        // sound isn't heard.
        let show = Rect::from_min_size(row.min + vec2(10.0, 24.0), vec2(58.0, 20.0));
        let mut shown = !track.muted;
        let tip = if track.kind.is_video() { "Seen in the video (untick to hide it)" } else { "Heard in the video (untick to mute it)" };
        if ui.put(show, egui::Checkbox::new(&mut shown, "Show")).on_hover_text(tip).changed() {
            self.run(ed, &ctx, "video.mute", json!({"track": i, "muted": !shown}));
        }
        if track.kind.is_video() {
            let device = self.rec.stream(track.stream).map(|s| s.name.clone()).unwrap_or_default();
            if !device.is_empty() && device != name {
                painter.text(row.min + vec2(74.0, 27.0), egui::Align2::LEFT_TOP, device, egui::FontId::proportional(11.0), t.muted);
            }
            return;
        }
        let vol = Rect::from_min_size(row.min + vec2(72.0, 24.0), vec2(row.width() - 80.0, 20.0));
        let mut v = track.volume * 100.0;
        let r = ui.put(vol, egui::Slider::new(&mut v, 0.0..=200.0).show_value(false)).on_hover_text(format!("Volume {v:.0}%"));
        if r.drag_started() {
            self.session.begin("Volume");
            self.volume_drag = true;
        }
        if r.changed() {
            let p = json!({"track": i, "volume": v / 100.0});
            if self.volume_drag {
                let _ = self.session.drag("video.volume", &p);
            } else {
                self.run(ed, &ctx, "video.volume", p)
            }
        }
        if r.drag_stopped() {
            self.session.commit();
            self.volume_drag = false;
        }
    }

    /// The captions' row on the timeline: a block for each line, in its speaker's track colour
    /// (faded when they're not on the picture), or where they stand.
    fn draw_captions_row(&self, painter: &egui::Painter, row: Rect, lane: Rect, pps: f32, scroll: f32, t: &Tokens) {
        let x_of = |us: u64| lane.min.x + (us as f64 / 1e6 * f64::from(pps)) as f32 - scroll;
        painter.rect_filled(row, 0.0, t.window);
        if self.cues.is_empty() {
            let status = if self.captions.is_some() {
                "The captions' tracks were cut or muted where they were said."
            } else if self.captions_coming.is_some() {
                "Making captions on this computer… they appear here when they're ready."
            } else if self.path.is_some() {
                "No captions yet. Make Captions writes down what was said, on this computer."
            } else {
                "Captions are made of recordings saved in a file."
            };
            painter.text(pos2(lane.min.x + 12.0, row.center().y), egui::Align2::LEFT_CENTER, status, egui::FontId::proportional(12.0), t.muted);
            return;
        }
        let faded = !self.session.timeline.captions.show;
        for c in self.cues.iter() {
            let (a, b) = (x_of(c.start), x_of(c.end));
            if b < lane.min.x || a > lane.max.x {
                continue;
            }
            let r = Rect::from_min_max(pos2(a, row.min.y + 3.0), pos2(b.max(a + 2.0), row.max.y - 3.0));
            let colour = if faded { Color32::from_gray(140) } else { kind_colour(c.speaker.source()) };
            painter.rect_filled(r, 4.0, colour.gamma_multiply(0.30));
            painter.rect_stroke(r, 4.0, Stroke::new(1.0, colour), StrokeKind::Inside);
            if r.width() > 28.0 {
                let inside = painter.with_clip_rect(r.shrink(3.0).intersect(lane));
                inside.text(r.min + vec2(6.0, 5.0), egui::Align2::LEFT_TOP, &c.text, egui::FontId::proportional(12.0), t.text);
                inside.text(r.min + vec2(6.0, 23.0), egui::Align2::LEFT_TOP, c.speaker.label(), egui::FontId::proportional(10.5), t.muted);
            }
        }
    }

    /// The captions' header: Show (on the picture) and In video (burned into the MP4), or Make
    /// Captions when there are none.
    fn captions_header(&mut self, ed: &mut Editor, ui: &mut Ui, row: Rect) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        ui.painter().rect_filled(Rect::from_min_size(row.min, vec2(4.0, row.height())), 0.0, Color32::from_gray(150));
        let title = Rect::from_min_size(row.min + vec2(14.0, 3.0), vec2(row.width() - 20.0, 18.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(title).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.label(RichText::new("Captions").size(13.0).color(t.text));
        });
        if self.captions.is_some() {
            let c = self.session.timeline.captions;
            let mut show = c.show;
            let r = ui.put(Rect::from_min_size(row.min + vec2(10.0, 24.0), vec2(60.0, 20.0)), egui::Checkbox::new(&mut show, "Show"));
            if r.on_hover_text("The captions on the picture here (Export Video asks whether they go in the video)").changed() {
                self.run(ed, &ctx, "video.captions", json!({"show": show}));
            }
        } else if self.captions_coming.is_some() {
            ui.painter_at(row).text(row.min + vec2(14.0, 26.0), egui::Align2::LEFT_TOP, "Being made…", egui::FontId::proportional(11.0), t.muted);
        } else if self.path.is_some() {
            let b = ui.put(Rect::from_min_size(row.min + vec2(10.0, 23.0), vec2(116.0, 21.0)), egui::Button::new("Make Captions"));
            if b.on_hover_text("Write down what was said, on this computer, in the background").clicked() {
                self.make_captions(ed, &ctx);
            }
        }
    }

    fn timeline(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let tracks = self.session.timeline.tracks.clone();
        // The captions' row, under the tracks, when there's sound to have words in.
        let caption_row = self.has_sound;
        let rows = tracks.len().max(1) + usize::from(caption_row);
        let height = RULER_H + rows as f32 * ROW_H + 10.0;
        egui::Panel::bottom("video_timeline")
            .exact_size(height)
            .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(0, 4)))
            .show(ui, |ui| {
                let full = ui.max_rect();
                let lane = Rect::from_min_max(pos2(full.min.x + HEADER_W, full.min.y), full.max);
                let length = self.length().max(1_000_000);
                if self.pps <= 0.0 {
                    self.pps = ((lane.width() - 24.0) / (length as f32 / 1e6)).clamp(2.0, 400.0);
                    self.scroll = 0.0;
                }
                let (pps, scroll) = (self.pps, self.scroll);
                let x_of = |us: u64| lane.min.x + (us as f64 / 1e6 * f64::from(pps)) as f32 - scroll;
                let us_of = |x: f32| (f64::from((x - lane.min.x + scroll).max(0.0)) / f64::from(pps) * 1e6) as u64;
                let row_rect =
                    |i: usize| Rect::from_min_size(pos2(full.min.x, full.min.y + RULER_H + i as f32 * ROW_H), vec2(full.width(), ROW_H - 4.0));
                // The lane takes the pointer first; the headers have their own controls.
                let resp = ui.interact(lane, ui.id().with("lane"), Sense::click_and_drag());
                let painter = ui.painter_at(full);
                // The ruler.
                let ruler = Rect::from_min_max(pos2(lane.min.x, full.min.y), pos2(lane.max.x, full.min.y + RULER_H));
                painter.rect_filled(ruler, 0.0, t.toolbar);
                let step = [1u64, 2, 5, 10, 15, 30, 60, 120, 300, 600].into_iter().find(|s| *s as f32 * pps >= 70.0).unwrap_or(1200);
                let first = us_of(lane.min.x) / 1_000_000 / step * step;
                let mut s = first;
                while x_of(s * 1_000_000) < lane.max.x {
                    let x = x_of(s * 1_000_000);
                    painter.line_segment([pos2(x, ruler.max.y - 8.0), pos2(x, ruler.max.y)], Stroke::new(1.0, t.muted));
                    painter.text(
                        pos2(x + 3.0, ruler.min.y + 4.0),
                        egui::Align2::LEFT_TOP,
                        format!("{}:{:02}", s / 60, s % 60),
                        egui::FontId::proportional(11.0),
                        t.muted,
                    );
                    s += step;
                }
                // The rows and their clips.
                for (i, track) in tracks.iter().enumerate() {
                    let row = row_rect(i);
                    let lane_row = Rect::from_min_max(pos2(lane.min.x, row.min.y), pos2(lane.max.x, row.max.y));
                    painter.rect_filled(lane_row, 0.0, if i % 2 == 0 { t.window } else { t.panel });
                    let colour = if track.muted { Color32::from_gray(140) } else { kind_colour(track.kind) };
                    for (c, clip) in track.clips.iter().enumerate() {
                        let r = Rect::from_min_max(pos2(x_of(clip.at), row.min.y + 3.0), pos2(x_of(clip.end()), row.max.y - 3.0));
                        if r.max.x < lane.min.x || r.min.x > lane.max.x {
                            continue;
                        }
                        painter.rect_filled(r, 4.0, colour.gamma_multiply(0.35));
                        painter.rect_stroke(r, 4.0, Stroke::new(1.0, colour), StrokeKind::Inside);
                        if let Some(peaks) = self.waves.get(&track.stream) {
                            let mid = r.center().y;
                            let half = r.height() / 2.0 - 3.0;
                            let (x0, x1) = (r.min.x.max(lane.min.x) as i32, r.max.x.min(lane.max.x) as i32);
                            for x in x0..x1 {
                                let (a, b) = (us_of(x as f32), us_of(x as f32 + 1.0));
                                let src = |us: u64| (clip.from + us.saturating_sub(clip.at)) * 48 / 1000 / WAVE_STEP;
                                let (i0, i1) = (src(a) as usize, (src(b) as usize).max(src(a) as usize + 1));
                                let (lo, hi) = peaks
                                    .get(i0..i1.min(peaks.len()))
                                    .unwrap_or_default()
                                    .iter()
                                    .fold((0i16, 0i16), |(l, h), (a, b)| (l.min(*a), h.max(*b)));
                                let g = track.volume.clamp(0.0, 2.0) / 32768.0 * half;
                                let (top, bottom) = ((mid - f32::from(hi) * g).max(r.min.y + 2.0), (mid - f32::from(lo) * g).min(r.max.y - 2.0));
                                painter
                                    .line_segment([pos2(x as f32 + 0.5, top), pos2(x as f32 + 0.5, bottom.max(top + 1.0))], Stroke::new(1.0, colour));
                            }
                        } else {
                            painter.text(
                                r.min + vec2(8.0, 6.0),
                                egui::Align2::LEFT_TOP,
                                track.kind.label(),
                                egui::FontId::proportional(12.0),
                                t.text,
                            );
                        }
                        if self.selected == Some((i, c)) {
                            painter.rect_stroke(r, 4.0, Stroke::new(2.5, t.accent), StrokeKind::Outside);
                        }
                    }
                }
                if caption_row {
                    let row = row_rect(tracks.len());
                    let lane_row = Rect::from_min_max(pos2(lane.min.x, row.min.y), pos2(lane.max.x, row.max.y));
                    self.draw_captions_row(&painter, lane_row, lane, pps, scroll, &t);
                }
                // The chosen stretch.
                if let Some((a, b)) = self.range {
                    let band = Rect::from_min_max(pos2(x_of(a), full.min.y), pos2(x_of(b), full.max.y));
                    painter.rect_filled(band, 0.0, t.accent.gamma_multiply(0.18));
                    painter.rect_stroke(band, 0.0, Stroke::new(1.0, t.accent), StrokeKind::Inside);
                }
                // The playhead.
                let px = x_of(self.playhead);
                if px >= lane.min.x && px <= lane.max.x {
                    let red = Color32::from_rgb(0xE5, 0x39, 0x35);
                    painter.line_segment([pos2(px, full.min.y), pos2(px, full.max.y)], Stroke::new(2.0, red));
                    painter.add(egui::Shape::convex_polygon(
                        vec![pos2(px - 6.0, full.min.y), pos2(px + 6.0, full.min.y), pos2(px, full.min.y + 9.0)],
                        red,
                        Stroke::NONE,
                    ));
                }
                // The headers, over the left of the rows.
                painter.rect_filled(Rect::from_min_max(full.min, pos2(lane.min.x, full.max.y)), 0.0, t.panel);
                painter.line_segment([pos2(lane.min.x, full.min.y), pos2(lane.min.x, full.max.y)], Stroke::new(1.0, t.border));
                for (i, track) in tracks.iter().enumerate() {
                    let row = row_rect(i);
                    self.header(ed, ui, Rect::from_min_max(row.min, pos2(lane.min.x - 1.0, row.max.y)), i, track);
                }
                if caption_row {
                    let row = row_rect(tracks.len());
                    self.captions_header(ed, ui, Rect::from_min_max(row.min, pos2(lane.min.x - 1.0, row.max.y)));
                }
                // The pointer on the lane.
                let row_at = |y: f32| -> Option<usize> {
                    let r = ((y - full.min.y - RULER_H) / ROW_H).floor();
                    (r >= 0.0 && (r as usize) < tracks.len()).then_some(r as usize)
                };
                let clip_at = |track: usize, x: f32| -> Option<(usize, Option<bool>)> {
                    let tr = tracks.get(track)?;
                    tr.clips.iter().enumerate().find_map(|(c, clip)| {
                        let (a, b) = (x_of(clip.at), x_of(clip.end()));
                        if x < a - EDGE || x > b + EDGE {
                            return None;
                        }
                        let edge = if (x - a).abs() <= EDGE {
                            Some(true)
                        } else if (x - b).abs() <= EDGE {
                            Some(false)
                        } else {
                            None
                        };
                        Some((c, edge))
                    })
                };
                if let Some(p) = resp.hover_pos()
                    && let Some(track) = row_at(p.y)
                    && let Some((_, Some(_))) = clip_at(track, p.x)
                {
                    ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                if resp.drag_started()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    self.drag = Some(if p.y < ruler.max.y {
                        Drag::Scrub
                    } else if let Some(track) = row_at(p.y) {
                        match clip_at(track, p.x) {
                            Some((clip, Some(start))) => {
                                self.selected = Some((track, clip));
                                self.session.begin("Trim");
                                Drag::Trim { track, clip, start }
                            }
                            Some((clip, None)) => {
                                self.selected = Some((track, clip));
                                self.session.begin("Move Clip");
                                let at = tracks.get(track).and_then(|tr| tr.clips.get(clip)).map(|c| c.at).unwrap_or(0);
                                Drag::Move { track, clip, grab: us_of(p.x).saturating_sub(at) }
                            }
                            None => Drag::Range(us_of(p.x).min(self.length())),
                        }
                    } else {
                        Drag::Range(us_of(p.x).min(self.length()))
                    });
                }
                if resp.dragged()
                    && let (Some(p), Some(d)) = (resp.interact_pointer_pos(), self.drag)
                {
                    let us = us_of(p.x);
                    match d {
                        Drag::Scrub => self.seek(ed, us, now),
                        Drag::Range(from) => {
                            let to = us.min(self.length());
                            self.range = (from.abs_diff(to) > 0).then_some((from.min(to), from.max(to)));
                            self.selected = None;
                        }
                        Drag::Trim { track, clip, start } => {
                            let id = if start { "video.trimStart" } else { "video.trimEnd" };
                            let _ = self.session.drag(id, &json!({"track": track, "clip": clip, "to": us}));
                        }
                        Drag::Move { track, clip, grab } => {
                            let _ = self.session.drag("video.moveClip", &json!({"track": track, "clip": clip, "to": us.saturating_sub(grab)}));
                        }
                    }
                    // Near the edges, the timeline scrolls along.
                    if p.x > lane.max.x - 20.0 {
                        self.scroll += 8.0;
                    } else if p.x < lane.min.x + 20.0 {
                        self.scroll = (self.scroll - 8.0).max(0.0);
                    }
                }
                if resp.drag_stopped() {
                    if matches!(self.drag, Some(Drag::Trim { .. } | Drag::Move { .. })) {
                        self.session.commit();
                    }
                    self.drag = None;
                }
                if resp.clicked()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    match row_at(p.y).and_then(|track| clip_at(track, p.x).map(|(c, _)| (track, c))) {
                        Some(sel) if p.y >= ruler.max.y => self.selected = Some(sel),
                        _ => {
                            self.selected = None;
                            self.range = None;
                            self.seek(ed, us_of(p.x), now);
                        }
                    }
                }
                // The wheel scrolls; with Ctrl it zooms around the pointer.
                if resp.hovered() {
                    let (dy, dx, zoom) = ctx.input(|i| (i.smooth_scroll_delta.y, i.smooth_scroll_delta.x, i.modifiers.command));
                    if zoom && dy != 0.0 {
                        let at = resp.hover_pos().map(|p| p.x).unwrap_or(lane.center().x);
                        let us = us_of(at);
                        self.pps = (self.pps * (1.0 + dy * 0.002)).clamp(2.0, 800.0);
                        self.scroll = ((us as f64 / 1e6 * f64::from(self.pps)) as f32 - (at - lane.min.x)).max(0.0);
                    } else if dy != 0.0 || dx != 0.0 {
                        self.scroll = (self.scroll - dy - dx).max(0.0);
                    }
                }
                // The playhead stays in view while playing.
                if self.playing.is_some() && (px > lane.max.x - 40.0 || px < lane.min.x) {
                    self.scroll = ((self.playhead as f64 / 1e6 * f64::from(self.pps)) as f32 - 60.0).max(0.0);
                }
                self.fix_selection();
            });
    }
}

/// Shows the video editor in the editor's window (when a recording is open).
pub fn show(ed: &mut Editor, ui: &mut Ui) {
    let Some(mut v) = ed.video.take() else { return };
    let epoch = ed.video_epoch;
    let ctx = ui.ctx().clone();
    v.keys(ed, &ctx);
    v.tick(ed, &ctx);
    v.menu_bar(ed, ui);
    v.tool_bar(ed, ui);
    v.status_bar(ui);
    if ed.settings.show_tray {
        crate::strip::show(ed, ui);
    }
    v.timeline(ed, ui);
    v.camera_panel(ed, ui);
    v.preview(ed, ui);
    v.export_windows(ed, &ctx);
    // Something else was opened meanwhile (from the strip): this one's edits are written first.
    if ed.video_epoch == epoch && ed.video.is_none() {
        ed.video = Some(v);
    } else {
        v.save_now(ed, &ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_reads_minutes_seconds_and_tenths() {
        assert_eq!(clock(0), "0:00.0");
        assert_eq!(clock(61_250_000), "1:01.2");
        assert_eq!(clock(3_600_000_000), "60:00.0");
    }
}
