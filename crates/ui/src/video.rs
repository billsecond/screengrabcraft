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

use egui::{Color32, Key, KeyboardShortcut, Modifiers, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use screengrabcraft_engine::VideoSession;
use screengrabcraft_media::captions::{self, Captions, Cue, Speaker};
use screengrabcraft_media::compose::{self, Composer};
use screengrabcraft_media::recording::{self, Recording, StreamKind};
use screengrabcraft_media::timeline::{CameraShape, Corner, Track};
use serde_json::{Value, json};

mod export_window;
mod library;
pub use library::{LibDrop, LibTab};

pub use export_window::{CaptionsIn, ExportChoice, ExportStep};

/// A layer item being moved or trimmed on the timeline: its layer (it may move to another, of
/// its kind; `from` is where it started), and where it was taken hold of (µs into it), or which
/// end.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ItemDrag {
    Move { layer: u32, item: u32, grab: u64, from: u32 },
    Trim { layer: u32, item: u32, start: bool },
}

/// The band above the layers that a drop on makes a layer of its own.
const NEW_LAYER_H: f32 = 22.0;

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
/// The whole-screen preview's bar of controls: how tall, and how long it stays once the pointer is
/// still while the video plays (seconds).
const FULL_BAR: f32 = 64.0;
const FULL_HIDE: f64 = 2.5;
/// How much larger than itself a small video is shown, at most.
const PREVIEW_ZOOM: f32 = 2.0;
/// How near (points) an edge on the timeline catches what's dragged past it.
const SNAP: f32 = 7.0;
/// How wide the panel at the right is, whatever it shows.
pub(crate) const SIDE_W: f32 = 264.0;
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
    pub pps: f32,
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
    /// The media bin's files as read, for the layers.
    pub library: Arc<screengrabcraft_media::media_store::MediaLibrary>,
    /// Files being brought in, and files of the bin being read again (a recording reopened).
    pub importing: Vec<library::Importing>,
    rereading: Vec<(u32, screengrabcraft_media::media_store::MediaJob)>,
    /// The layer item chosen (its layer, itself).
    pub item: Option<(u32, u32)>,
    /// The camera chosen on the picture (to move and resize it there).
    pub camera_chosen: bool,
    /// The library panel at the left: shown, and which tab.
    pub library_open: bool,
    pub lib_tab: LibTab,
    /// The library's thumbnails, by template or media.
    thumbs: std::collections::HashMap<String, egui::TextureHandle>,
    /// Something being moved or resized on the picture.
    canvas_drag: Option<library::CanvasDrag>,
    /// A layer item being moved or trimmed on the timeline.
    item_drag: Option<ItemDrag>,
    /// Where the picture was last shown (screen points).
    pub picture_rect: Option<Rect>,
    /// Where the timeline's lane was last drawn, and where its rows start (for finding things on it).
    pub lane_rect: Option<(Rect, f32)>,
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
    /// How far the recording's captions being made have got (0 to 1), as the service says.
    pub captions_progress: Option<f32>,
    /// The lines of the sound files and videos in the media bin (each in its own time), by id.
    pub media_cues: std::collections::HashMap<u32, Vec<Cue>>,
    /// The host's time of each one's captions last seen.
    media_seen: std::collections::HashMap<u32, u64>,
    /// Those being made: since when, and how far they've got.
    pub media_coming: std::collections::HashMap<u32, (f64, Option<f32>)>,
    /// The host's time of the captions file last seen, and when to ask again.
    captions_seen: u64,
    next_poll: f64,
    /// The preview over the whole screen, with nothing but its controls.
    pub full_screen: bool,
    /// Where the pointer last was there, and when it last moved: the controls go away while the
    /// video plays and the pointer is still.
    full_pointer: Option<Pos2>,
    full_moved: f64,
    /// The timeline asked to zoom by so much (a button, a key, a menu): done when it's next drawn,
    /// around the playhead.
    /// What the library is searched for (its cards with these words in their names).
    pub lib_search: String,
    zoom_ask: Option<f32>,
    /// Where on the timeline the right button was pressed (µs): what its menu is about.
    menu_us: Option<u64>,
}

/// A mark at each end of what's chosen on the timeline: its ends are there to be dragged.
fn grips(painter: &egui::Painter, r: Rect) {
    if r.width() < 24.0 {
        return;
    }
    let tall = (r.height() * 0.45).clamp(8.0, 18.0);
    for x in [r.min.x + 4.5, r.max.x - 4.5] {
        let mark = Rect::from_center_size(pos2(x, r.center().y), vec2(3.0, tall));
        painter.rect_filled(mark, 1.5, Color32::from_white_alpha(230));
        painter.rect_stroke(mark, 1.5, Stroke::new(0.5, Color32::from_black_alpha(90)), StrokeKind::Outside);
    }
}

/// A sound's volume in its header: a slider, and what it is set to beside it.
fn volume_slider(ui: &mut Ui, rect: Rect, v: &mut f32) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    ui.painter().text(
        pos2(rect.max.x - 2.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        format!("{:.0}%", *v),
        egui::FontId::proportional(11.0),
        t.muted,
    );
    let slider = Rect::from_min_max(rect.min, pos2(rect.max.x - 36.0, rect.max.y));
    let was = *v;
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = slider.width().max(40.0);
        ui.put(slider, egui::Slider::new(v, 0.0..=200.0).show_value(false))
    })
    .inner
    .on_hover_text(format!("Volume {was:.0}% (100% is as it was recorded)"))
}

/// What a header's switch turns on and off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Switch {
    /// A picture: seen in the video, or hidden.
    Picture,
    /// A sound: heard in the video, or muted.
    Sound,
    /// The captions, on the picture here.
    Captions,
}

/// The switch in a track's or a layer's header: an eye for a picture (seen in the video, or
/// hidden), a speaker for a sound (heard, or muted). True when it was pressed. Screen readers (and
/// tests) know it as the check box Show.
fn track_switch(ui: &mut Ui, salt: (&str, usize), rect: Rect, what: Switch, shown: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let resp = ui.interact(rect, ui.id().with(("track-switch", salt)), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, shown, "Show"));
    let (icon, tip) = match (what, shown) {
        (Switch::Picture, true) => (Icon::Eye, "Seen in the video. Click to hide it."),
        (Switch::Picture, false) => (Icon::EyeOff, "Hidden: not in the video. Click to show it."),
        (Switch::Sound, true) => (Icon::Speaker, "Heard in the video. Click to mute it."),
        (Switch::Sound, false) => (Icon::SpeakerOff, "Muted: not heard in the video. Click to hear it."),
        (Switch::Captions, true) => {
            (Icon::Eye, "The captions are on the picture here. Click to take them off. (Export Video asks whether they go in the video.)")
        }
        (Switch::Captions, false) => (Icon::EyeOff, "The captions are not on the picture. Click to show them."),
    };
    if resp.hovered() {
        ui.painter().rect_filled(rect, 5.0, t.hover);
    }
    let colour = if !shown {
        t.danger
    } else if resp.hovered() {
        t.text
    } else {
        t.muted
    };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(17.0, 17.0)), icon, colour);
    resp.on_hover_text(tip).clicked()
}

/// A heading for a group of settings in the panel at the right: a line above it (unless it is the
/// panel's first), then its name, as the picture editor's panel has them.
pub(crate) fn section(ui: &mut Ui, name: &str, first: &mut bool) {
    if !*first {
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(4.0);
    }
    *first = false;
    ui.label(RichText::new(name).strong());
    ui.add_space(4.0);
}

/// A slider with its name small above it, as wide as the panel leaves beside its number box.
pub(crate) fn named_slider(ui: &mut Ui, name: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(name).size(12.0).color(t.muted));
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 76.0).max(80.0);
        ui.add(egui::Slider::new(value, range).fixed_decimals(0).suffix(suffix))
    })
    .inner
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

/// Each sound track's peaks per 10 ms, for drawing its wave.
pub(crate) type Waves = BTreeMap<u8, Vec<(i16, i16)>>;

/// The samples of a sound track that are looked at for its wave: the first two hours at most.
fn wave_samples(rec: &Recording, id: u8) -> u64 {
    compose::to_samples(rec.stream_duration(id)).min(48_000 * 7200)
}

/// How many samples [`waves`] looks at in all (to say how far it has got).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn wave_work(rec: &Recording) -> u64 {
    rec.header.streams.iter().filter(|s| !s.kind.is_video()).map(|s| wave_samples(rec, s.id)).sum()
}

/// Each sound track's peaks per 10 ms (for drawing its wave), the first two hours at most;
/// `done` is told how many samples have been looked at so far.
pub(crate) fn waves(rec: &Recording, done: &dyn Fn(u64)) -> Waves {
    let mut out = BTreeMap::new();
    let mut so_far = 0u64;
    for s in rec.header.streams.iter().filter(|s| !s.kind.is_video()) {
        let total = wave_samples(rec, s.id);
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
            so_far += n as u64;
            done(so_far);
        }
        out.insert(s.id, peaks);
    }
    out
}

impl VideoView {
    pub fn new(rec: Recording, path: Option<String>, name: String) -> VideoView {
        let waves = waves(&rec, &|_| {});
        VideoView::with_waves(rec, waves, path, name)
    }

    /// [`VideoView::new`], its sound's waves worked out already (on another thread).
    pub(crate) fn with_waves(rec: Recording, waves: Waves, path: Option<String>, name: String) -> VideoView {
        let session = VideoSession::new(&rec);
        let (w, h) = rec
            .header
            .streams
            .iter()
            .find(|s| s.kind == StreamKind::Screen)
            .or_else(|| rec.header.streams.iter().find(|s| s.kind == StreamKind::Camera))
            .map(|s| compose::fit(s.width, s.height, PREVIEW_MAX.0, PREVIEW_MAX.1))
            .unwrap_or((1280, 720));
        let captions = rec.captions.clone().filter(|c| !c.is_empty());
        let has_sound = rec.header.streams.iter().any(|s| Speaker::of(s.kind).is_some());
        let rec = Arc::new(rec);
        let library = Arc::new(screengrabcraft_media::media_store::MediaLibrary::default());
        let mut composer = Composer::new(rec.clone(), &session.timeline, w, h);
        composer.set_media(library.clone());
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
            library,
            importing: Vec::new(),
            rereading: Vec::new(),
            item: None,
            camera_chosen: false,
            library_open: true,
            lib_tab: LibTab::default(),
            thumbs: Default::default(),
            canvas_drag: None,
            item_drag: None,
            picture_rect: None,
            lane_rect: None,
            waves,
            volume_drag: false,
            captions,
            captions_rev: 0,
            cues: Arc::new(Vec::new()),
            cues_for: None,
            has_sound,
            captions_coming: None,
            captions_progress: None,
            media_cues: Default::default(),
            media_seen: Default::default(),
            media_coming: Default::default(),
            captions_seen: 0,
            next_poll: 0.0,
            full_screen: false,
            full_pointer: None,
            full_moved: 0.0,
            lib_search: String::new(),
            zoom_ask: None,
            menu_us: None,
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
        self.cues = Arc::new(screengrabcraft_media::captions::edited_cues(self.captions.as_ref(), &self.session.timeline, &self.media_cues));
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
        self.transcribe(ed, ctx, false);
    }

    /// The sound files and videos in the media bin with sound to hear: id, name, path.
    fn media_with_sound(&self) -> Vec<(u32, String, String)> {
        use screengrabcraft_media::layers::MediaKind;
        self.session
            .timeline
            .media
            .iter()
            .filter(|m| m.kind != MediaKind::Picture && !m.path.is_empty() && self.library.has_sound(m.id) != Some(false))
            .map(|m| (m.id, m.name.clone(), m.path.clone()))
            .collect()
    }

    /// Whether anything in the video can be heard: the recording, or what was put on its layers.
    fn hears_anything(&self) -> bool {
        self.has_sound || !self.media_with_sound().is_empty()
    }

    /// Whether there's something to transcribe: the recording (from a file) or its media.
    fn can_transcribe(&self) -> bool {
        (self.has_sound && self.path.is_some()) || !self.media_with_sound().is_empty()
    }

    /// Transcribe (or Transcribe Again, `again`): what's said in the video, on this computer, in
    /// the background: the recording's tracks, and the sound files and videos put on its layers
    /// (each in its own time, so its lines follow it wherever it's put).
    pub fn transcribe(&mut self, ed: &mut Editor, ctx: &egui::Context, again: bool) {
        let now = ctx.input(|i| i.time);
        let mut asked = false;
        if let Some(path) = self.path.clone().filter(|_| self.has_sound) {
            let r = if again { ed.services.make_captions_again(&path) } else { ed.services.make_captions(&path) };
            match r {
                Ok(()) => {
                    self.expect_captions(now);
                    self.captions_progress = None;
                    asked = true;
                }
                Err(e) => {
                    ed.error(ctx, e);
                    return;
                }
            }
        }
        for (id, name, path) in self.media_with_sound() {
            if self.library.has_sound(id) != Some(true) || (!again && self.media_cues.contains_key(&id)) {
                continue;
            }
            let r = if again { ed.services.make_captions_again(&path) } else { ed.services.make_captions(&path) };
            match r {
                Ok(()) => {
                    self.media_coming.insert(id, (now, None));
                    asked = true;
                }
                Err(e) => ed.error(ctx, format!("{name}: {e}")),
            }
        }
        if asked {
            self.next_poll = now;
        } else if self.path.is_none() && self.has_sound {
            ed.error(ctx, "Only a recording opened from a file can be transcribed");
        }
    }

    /// A file put in the media bin: transcribed by itself when Settings says so.
    pub(crate) fn media_added(&mut self, ed: &mut Editor, ctx: &egui::Context, id: u32, path: &str, hears: bool) {
        if !hears || path.is_empty() || !ed.services.auto_captions() {
            return;
        }
        match ed.services.make_captions(path) {
            Ok(()) => {
                let now = ctx.input(|i| i.time);
                self.media_coming.insert(id, (now, None));
                self.next_poll = now;
            }
            Err(e) => log::warn!("transcribing what was brought in: {e}"),
        }
    }

    /// What's being transcribed, for the status bar and the captions' header: what, and how far
    /// it has got.
    pub fn transcribing(&self) -> Option<(String, Option<f32>)> {
        if self.captions_coming.is_some() {
            return Some(("Transcribing…".to_string(), self.captions_progress));
        }
        let (id, (_, p)) = self.media_coming.iter().min_by_key(|(id, _)| **id)?;
        let name = self.session.timeline.media.iter().find(|m| m.id == *id).map(|m| m.name.clone()).unwrap_or_default();
        Some((format!("Transcribing {name}…"), *p))
    }

    /// The media bin's captions: those being made (how far, and whether they've come), and now
    /// and then those made meanwhile (a recording opened again finds its media's).
    fn poll_media_captions(&mut self, ed: &mut Editor, ctx: &egui::Context, now: f64) {
        let media = self.media_with_sound();
        for (id, name, path) in &media {
            let coming = self.media_coming.get(id).copied();
            if coming.is_some() {
                match ed.services.captions_progress(path) {
                    Some(Ok(p)) => {
                        self.media_coming.insert(*id, (now, Some(p)));
                    }
                    Some(Err(why)) => {
                        self.media_coming.remove(id);
                        ed.error(ctx, format!("{name} couldn't be transcribed: {why}"));
                        continue;
                    }
                    None => {}
                }
            }
            let seen = self.media_seen.get(id).copied().unwrap_or(0);
            if let Some((s, cues)) = ed.services.poll_media_captions(path, seen) {
                self.media_seen.insert(*id, s);
                let came = self.media_coming.remove(id).is_some();
                if came {
                    ed.toast(
                        ctx,
                        if cues.is_empty() { format!("No words were heard in {name}") } else { format!("{name} transcribed: {} lines", cues.len()) },
                    );
                }
                self.media_cues.insert(*id, cues);
                self.captions_rev += 1;
            } else if let Some((since, _)) = coming
                && now - since > CAPTIONS_WAIT
            {
                self.media_coming.remove(id);
            } else {
                // Looked for: from now on now and then, unless they're coming.
                self.media_seen.entry(*id).or_insert(0);
            }
        }
        // Taken out of the bin: its lines go too.
        let before = self.media_cues.len();
        self.media_cues.retain(|id, _| media.iter().any(|(m, _, _)| m == id));
        self.media_coming.retain(|id, _| media.iter().any(|(m, _, _)| m == id));
        if self.media_cues.len() != before {
            self.captions_rev += 1;
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
                    ed.toast(ctx, "Transcribing first, on this computer: you'll choose where the transcript goes when it's ready");
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
        let waiting = self.waiting_for_captions();
        let media = !self.media_coming.is_empty() || self.media_with_sound().iter().any(|(id, _, _)| !self.media_seen.contains_key(id));
        if !waiting && !media && self.media_with_sound().is_empty() {
            return;
        }
        if let Some(since) = self.captions_coming
            && now - since > CAPTIONS_WAIT
        {
            self.captions_coming = None;
        }
        let coming = self.captions_coming.is_some() || !self.media_coming.is_empty();
        if coming {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(CAPTIONS_POLL));
        }
        if now < self.next_poll {
            return;
        }
        // Often while they're coming; now and then otherwise (made by the service meanwhile).
        self.next_poll = now + if coming || media { CAPTIONS_POLL } else { CAPTIONS_GLANCE };
        self.poll_media_captions(ed, ctx, now);
        if !waiting {
            return;
        }
        // How far the recording's have got, or why they couldn't be made.
        if self.captions_coming.is_some()
            && let Some(path) = self.path.clone()
        {
            match ed.services.captions_progress(&path) {
                Some(Ok(p)) => {
                    self.captions_progress = Some(p);
                    // Still at it: not given up on.
                    self.captions_coming = Some(now);
                }
                Some(Err(why)) => {
                    self.captions_coming = None;
                    self.captions_progress = None;
                    self.transcript_after = None;
                    ed.error(ctx, format!("The recording couldn't be transcribed: {why}"));
                }
                None => {}
            }
        }
        {
            if let Some(path) = self.path.clone()
                && let Some((seen, caps)) = ed.services.poll_captions(&path, self.captions_seen)
            {
                self.captions_progress = None;
                self.captions_seen = seen;
                // The service has written its answer: some lines, or none heard.
                let came = self.captions_coming.take().is_some();
                match caps.filter(|c| !c.is_empty()) {
                    Some(c) => {
                        let n = c.cues.len();
                        self.captions = Some(c);
                        self.captions_rev += 1;
                        if came {
                            ed.toast(ctx, format!("Transcribed: {n} lines"));
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
        let sound = ed.services.play_sound(self.rec.clone(), self.session.timeline.clone(), self.library.clone(), self.playhead);
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
        } else if let Some((layer, item)) = self.item.take() {
            self.run(ed, ctx, "video.deleteItem", json!({"layer": layer, "item": item}));
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
        self.poll_media(ed, ctx);
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
        if plain(Key::F11) || plain(Key::F) {
            self.set_full_screen(!self.full_screen, now);
        }
        // Over the whole screen it's a player: its keys play and seek, and Esc comes back.
        if self.full_screen {
            if plain(Key::Escape) {
                self.set_full_screen(false, now);
            }
            if plain(Key::Space) {
                if self.playing.is_some() { self.pause() } else { self.play(ed, now) }
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
            return;
        }
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
        if pressed(cmd(Key::Plus)) || pressed(cmd(Key::Equals)) {
            self.zoom_ask = Some(1.25);
        }
        if pressed(cmd(Key::Minus)) {
            self.zoom_ask = Some(0.8);
        }
        if pressed(cmd(Key::Num0)) {
            self.pps = 0.0;
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
                    if ui.button("Import Media…").on_hover_text("Pictures, sound files and recordings for the Media Bin").clicked() {
                        ed.services.request_import();
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
                    if self.hears_anything() {
                        let can = self.can_transcribe() && self.transcribing().is_none();
                        let again = self.captions.is_some() || !self.media_cues.is_empty();
                        let label = if again { "Transcribe Again" } else { "Transcribe" };
                        if ui
                            .add_enabled(can, egui::Button::new(label))
                            .on_hover_text(
                                "Write down what's said in the video, on this computer, in the background (the sound and videos you added included)",
                            )
                            .clicked()
                        {
                            self.transcribe(ed, &ctx, again);
                            ui.close();
                        }
                    }
                    if ed.services.has_paths()
                        && ui
                            .button("Transcribe a Video or Sound File…")
                            .on_hover_text("Any video or sound file, made by another program too: a transcript and subtitles beside it")
                            .clicked()
                    {
                        ed.services.request_transcribe();
                        ui.close();
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
                    let what = if self.item.is_some() {
                        "Delete the Item"
                    } else if self.range.is_some() {
                        "Cut the Stretch"
                    } else {
                        "Delete the Clip"
                    };
                    let any = self.item.is_some() || self.range.is_some() || self.selected.is_some();
                    if ui.add_enabled(any, egui::Button::new(what).shortcut_text("Del")).clicked() {
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
                    ui.checkbox(&mut self.library_open, "Library and Media Bin");
                    if ui.checkbox(&mut ed.settings.show_tray, "Recent Captures").changed() {
                        ed.services.save_settings(&ed.settings);
                    }
                    ui.separator();
                    if ui.add(egui::Button::new("Zoom the Timeline In").shortcut_text("Ctrl++")).clicked() {
                        self.zoom_ask = Some(1.25);
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Zoom the Timeline Out").shortcut_text("Ctrl+-")).clicked() {
                        self.zoom_ask = Some(0.8);
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Fit the Timeline").shortcut_text("Ctrl+0")).clicked() {
                        self.pps = 0.0;
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Full Screen Preview").shortcut_text("F11")).clicked() {
                        let now = ui.ctx().input(|i| i.time);
                        self.set_full_screen(true, now);
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
                    if ui.button("Keyboard Shortcuts").clicked() {
                        ed.dialog = Some(crate::dialogs::Dialog::VideoShortcuts);
                        ui.close();
                    }
                    if ui.button(format!("About {}", screengrabcraft_model::EDITOR_NAME)).clicked() {
                        ed.dialog = Some(crate::dialogs::Dialog::About);
                        ui.close();
                    }
                });
            });
        });
    }

    /// The tool bar, the picture editor's: as tall, built of the same buttons, and like it with a
    /// name under each button when the window is wide enough for that one's names (else Split
    /// and Delete are named beside their icons). The library and files at the left, then undo,
    /// then what cuts the video; what makes something of it at the right. (Playing is under the
    /// picture.)
    fn tool_bar(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let side = crate::editor::BAR_BUTTON;
        egui::Panel::top("video_tools")
            .frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(egui::Margin::symmetric(8, 4)))
            .show_separator_line(true)
            .show(ui, |ui| {
                let named = !ed.settings.hide_tool_names && ui.available_width() >= ed.named_bar_width;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                    let divider = |ui: &mut Ui| {
                        ui.add_space(4.0);
                        let (line, _) = ui.allocate_exact_size(vec2(1.0, side), Sense::hover());
                        ui.painter().vline(line.center().x, line.top() + side * 0.15..=line.top() + side * 0.85, Stroke::new(1.0, t.border));
                        ui.add_space(4.0);
                    };
                    // One of the bar's buttons: named under its icon, or (the few better said
                    // than drawn) beside it, or the icon alone.
                    let button = |ui: &mut Ui, icon: Icon, name: &str, known_as: &str, tip: &str, on: bool, enabled: bool, said: bool| {
                        if named {
                            icons::named_button(ui, icon, name, known_as, tip, on, enabled, side)
                        } else if said {
                            icons::labeled_button(ui, icon, known_as, tip, on, enabled, side)
                        } else {
                            icons::button_sized(ui, icon, known_as, on, enabled, side)
                        }
                    };
                    let tip = "Titles, lower thirds, overlays, transitions and your own media, to drag onto the timeline";
                    if button(ui, Icon::Library, "Library", "Library", tip, self.library_open, true, true).clicked() {
                        self.library_open = !self.library_open;
                    }
                    if button(ui, Icon::Open, "Open", "Open (Ctrl+O)", "Open (Ctrl+O)", false, true, false).clicked() {
                        ed.services.request_open();
                    }
                    divider(ui);
                    let undo_tip = format!("Undo {} (Ctrl+Z)", self.session.undo_label().unwrap_or(""));
                    if button(ui, Icon::Undo, "Undo", &undo_tip, &undo_tip, false, self.session.can_undo(), false).clicked() {
                        self.run(ed, &ctx, "edit.undo", json!({}));
                    }
                    let redo_tip = format!("Redo {} (Ctrl+Y)", self.session.redo_label().unwrap_or(""));
                    if button(ui, Icon::Redo, "Redo", &redo_tip, &redo_tip, false, self.session.can_redo(), false).clicked() {
                        self.run(ed, &ctx, "edit.redo", json!({}));
                    }
                    divider(ui);
                    let tip = "Split at the playhead (S): the selected clip's track, or all of them";
                    if button(ui, Icon::Split, "Split", "Split", tip, false, self.length() > 0, true).clicked() {
                        self.split(ed, &ctx);
                    }
                    let tip = if self.item.is_some() {
                        "Take the chosen item off its layer (Del)"
                    } else if self.range.is_some() {
                        "Take the chosen stretch out of every track (Del)"
                    } else {
                        "Delete the selected clip, leaving a gap (Del)"
                    };
                    let any = self.item.is_some() || self.range.is_some() || self.selected.is_some();
                    let label = if self.range.is_some() { "Cut" } else { "Delete" };
                    if button(ui, Icon::Delete, label, label, tip, false, any, true).clicked() {
                        self.delete(ed, &ctx);
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
                                "Transcribing first, on this computer…"
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

    fn status_bar(&self, ui: &mut Ui, files: Option<(String, Option<f32>)>) {
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
                    if !self.cues.is_empty() {
                        ui.label(small(format!("{} caption lines", self.cues.len())));
                    }
                    let saved = if self.session.revision() == self.saved { "Edits saved" } else { "Saving…" };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(small(saved.to_string()));
                        // Transcribing: this video's, else files picked to transcribe.
                        if let Some((text, p)) = self.transcribing().or_else(|| files.clone()) {
                            crate::transcribe::indicator(ui, &text, p);
                        }
                    });
                });
            });
    }

    /// The panel at the right while nothing on the timeline is chosen: the camera and the pointer
    /// (when the recording has them), and how the timeline is worked. Always there, so the picture
    /// doesn't jump about as things are chosen.
    fn camera_panel(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let camera = self.session.timeline.find(StreamKind::Camera);
        let pointer = !self.rec.pointer.is_empty();
        let t = Tokens::get(ui.ctx());
        egui::Panel::right("video_camera")
            .exact_size(SIDE_W)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::same(14)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.label(RichText::new("Video").size(16.0).color(t.text));
                    ui.label(RichText::new("Click a clip or an item on the timeline to change it.").size(12.0).color(t.muted));
                    ui.add_space(10.0);
                    let mut first = true;
                    if let Some(track) = camera {
                        section(ui, "Camera", &mut first);
                        self.camera_section(ed, ui, track);
                    }
                    if pointer {
                        section(ui, "Pointer", &mut first);
                        self.pointer_section(ed, ui);
                    }
                    section(ui, "Editing", &mut first);
                    for (keys, what) in [
                        ("Click", "the ruler to move the playhead; a clip or an item to select it."),
                        ("Drag", "a clip's end to trim it, the clip itself to slide it, or across the tracks to choose a stretch."),
                        ("S", "splits at the playhead."),
                        ("Delete", "takes out what's selected, or the chosen stretch from every track."),
                        ("Right-click", "the timeline for more."),
                    ] {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.label(RichText::new(keys).size(12.0).strong().color(t.text));
                            ui.label(RichText::new(what).size(12.0).color(t.muted));
                        });
                        ui.add_space(2.0);
                    }
                });
            });
    }

    /// The mouse pointer on the video: shown or not, how large, how smooth, its clicks ringed.
    fn pointer_section(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let look = self.session.timeline.pointer;
        let mut show = look.show;
        if ui.checkbox(&mut show, "Show the mouse pointer").on_hover_text("The pointer as it was while recording").changed() {
            self.run(ed, &ctx, "video.pointer", json!({"show": show}));
        }
        ui.add_enabled_ui(look.show, |ui| {
            for (label, key, value, range, tip) in [
                ("Size", "size", look.size * 100.0, 50.0..=400.0, "How large it's drawn: 100% is as it was on the screen"),
                (
                    "Smoother movement",
                    "smooth",
                    look.smooth * 100.0,
                    0.0..=100.0,
                    "Evens out its path. Clicks and drags stay exactly where they were",
                ),
            ] {
                ui.add_space(6.0);
                let mut v = value;
                let r = named_slider(ui, label, &mut v, range, "%").on_hover_text(tip);
                if r.drag_started() {
                    self.session.begin("Pointer");
                }
                if r.changed() {
                    let p = json!({ key: v / 100.0 });
                    if r.dragged() {
                        let _ = self.session.drag("video.pointer", &p);
                    } else {
                        self.run(ed, &ctx, "video.pointer", p)
                    }
                }
                if r.drag_stopped() {
                    self.session.commit();
                }
            }
            ui.add_space(6.0);
            let mut clicks = look.clicks;
            if ui
                .checkbox(&mut clicks, "Show clicks")
                .on_hover_text("A ring where a button is pressed: yellow for the left, blue for the right")
                .changed()
            {
                self.run(ed, &ctx, "video.pointer", json!({"clicks": clicks}));
            }
        });
    }

    /// The camera's place in the picture: its corner, its shape, its size.
    fn camera_section(&mut self, ed: &mut Editor, ui: &mut Ui, track: usize) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let cam = self.session.timeline.camera;
        let small = |s: &str| RichText::new(s).size(12.0).color(t.muted);
        ui.label(small("Corner"));
        let half = ((ui.available_width() - 6.0) / 2.0).floor();
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
                if ui.add(egui::Button::new(label).selected(cam.corner == corner).min_size(vec2(half, 28.0))).clicked() {
                    self.run(ed, &ctx, "video.camera", json!({"corner": corner}));
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(8.0);
        ui.label(small("Shape"));
        let third = ((ui.available_width() - 12.0) / 3.0).floor();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (shape, label) in [(CameraShape::Rectangle, "Square"), (CameraShape::Rounded, "Rounded"), (CameraShape::Circle, "Circle")] {
                if ui.add(egui::Button::new(label).selected(cam.shape == shape).min_size(vec2(third, 28.0))).clicked() {
                    self.run(ed, &ctx, "video.camera", json!({"shape": shape}));
                }
            }
        });
        ui.add_space(8.0);
        let mut size = cam.size * 100.0;
        let r = named_slider(ui, "Size", &mut size, 10.0..=50.0, "%").on_hover_text("Its width, as a share of the video's");
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
        ui.add_space(4.0);
        ui.label(small("Or drag the camera on the picture to put it anywhere."));
        let hidden = self.session.timeline.tracks.get(track).is_some_and(|tr| tr.clips.is_empty());
        if hidden {
            ui.add_space(4.0);
            ui.label(RichText::new("The camera's clips were all deleted.").color(t.muted));
        }
    }

    /// The picture at the playhead, as a texture: made again when the playhead or the edits move.
    fn refresh_frame(&mut self, ctx: &egui::Context) {
        let key = (self.playhead, self.session.revision());
        if self.shown != Some(key) {
            let pic = self.composer.frame(&self.session.timeline, self.playhead);
            let img = egui::ColorImage::from_rgba_unmultiplied([pic.width() as usize, pic.height() as usize], pic.pixels());
            match &mut self.texture {
                Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
                None => self.texture = Some(ctx.load_texture("video-frame", img, egui::TextureOptions::LINEAR)),
            }
            self.shown = Some(key);
        }
    }

    /// Puts the preview over the whole screen, or brings it back. (The window itself follows:
    /// `Editor::ui`.) Nothing is being dragged or chosen on the picture there.
    pub fn set_full_screen(&mut self, full: bool, now: f64) {
        if full && self.export.is_some() {
            return;
        }
        self.full_screen = full;
        self.full_moved = now;
        self.canvas_drag = None;
        if full {
            self.picture_rect = None;
        }
    }

    /// The preview over the whole screen: the picture as large as it goes, on black, and along
    /// the bottom a bar to play, seek and come back, which goes away while the video plays and
    /// the pointer is still. A click on the picture plays or pauses; a double click comes back.
    fn full_preview(&mut self, ed: &Editor, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        self.refresh_frame(&ctx);
        let now = ctx.input(|i| i.time);
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(ui, |ui| {
            // The whole window, whatever room the panel was given.
            let whole = ctx.viewport_rect();
            ui.set_clip_rect(whole);
            ui.painter().rect_filled(whole, 0.0, Color32::BLACK);
            if let Some(tex) = &self.texture {
                let size = tex.size_vec2();
                let f = (whole.width() / size.x).min(whole.height() / size.y).max(0.01);
                let r = Rect::from_center_size(whole.center(), size * f);
                ui.painter().image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            let playing = self.playing.is_some();
            let length = self.length();
            let toggle = |v: &mut VideoView| {
                if v.playing.is_some() {
                    v.pause();
                } else if length > 0 {
                    v.play(ed, now);
                }
            };
            let bar = Rect::from_min_max(pos2(whole.min.x, (whole.max.y - FULL_BAR).max(whole.min.y)), whole.max);
            // The picture itself: a click plays or pauses, a double click comes back.
            let stage = Rect::from_min_max(whole.min, pos2(whole.max.x, bar.min.y));
            let on_stage = ui.interact(stage, ui.id().with("full-stage"), Sense::click());
            if on_stage.double_clicked() {
                // Its first click played or paused: as it was before, then back.
                toggle(self);
                self.set_full_screen(false, now);
                return;
            }
            if on_stage.clicked() {
                toggle(self);
            }
            let pointer = ctx.input(|i| i.pointer.latest_pos());
            if pointer != self.full_pointer {
                self.full_pointer = pointer;
                self.full_moved = now;
            }
            let on_bar = pointer.is_some_and(|p| bar.contains(p));
            let since = now - self.full_moved;
            if playing && !on_bar && since >= FULL_HIDE {
                // Nothing but the picture: the pointer goes too.
                ctx.set_cursor_icon(egui::CursorIcon::None);
                return;
            }
            if playing {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64((FULL_HIDE - since).clamp(0.05, FULL_HIDE)));
            }
            let painter = ui.painter().clone();
            painter.rect_filled(bar, 0.0, Color32::from_black_alpha(200));
            let mid = bar.center().y;
            // Play or pause.
            let play = Rect::from_center_size(pos2(bar.min.x + 20.0 + PLAY / 2.0, mid), vec2(PLAY, PLAY));
            let tip = if playing { "Pause (Space)" } else { "Play (Space)" };
            let resp = ui.interact(play, ui.id().with("full-play"), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, length > 0, tip));
            painter.circle_filled(play.center(), PLAY / 2.0, if resp.hovered() { t.accent.gamma_multiply(1.15) } else { t.accent });
            icons::paint(&painter, play.shrink(PLAY * 0.27), if playing { Icon::Pause } else { Icon::Play }, Color32::WHITE);
            if resp.on_hover_text(tip).clicked() {
                toggle(self);
            }
            // Back from the whole screen, at the other end.
            let exit = Rect::from_center_size(pos2(bar.max.x - 20.0 - 18.0, mid), vec2(36.0, 36.0));
            let resp = ui.interact(exit, ui.id().with("full-exit"), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Exit Full Screen (Esc)"));
            if resp.hovered() {
                painter.rect_filled(exit, 6.0, Color32::from_white_alpha(36));
            }
            icons::paint(&painter, exit.shrink(8.0), Icon::ExitFullScreen, Color32::WHITE);
            if resp.on_hover_text("Exit Full Screen (Esc)").clicked() {
                self.set_full_screen(false, now);
                return;
            }
            // The time, and between it and the way back the whole video to seek along.
            let clock_text = format!("{} / {}", clock(self.playhead), clock(length));
            let words =
                painter.text(pos2(play.max.x + 14.0, mid), egui::Align2::LEFT_CENTER, clock_text, egui::FontId::monospace(14.0), Color32::WHITE);
            let track = Rect::from_min_max(pos2(words.max.x + 18.0, mid - 3.0), pos2(exit.min.x - 18.0, mid + 3.0));
            if track.width() >= 40.0 && length > 0 {
                let resp = ui.interact(track.expand2(vec2(0.0, 12.0)), ui.id().with("full-seek"), Sense::click_and_drag());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, "Seek"));
                if (resp.dragged() || resp.clicked() || resp.drag_started())
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    let at = ((p.x - track.min.x) / track.width()).clamp(0.0, 1.0);
                    self.seek(ed, (f64::from(at) * length as f64) as u64, now);
                }
                let done = (self.playhead as f64 / length as f64).clamp(0.0, 1.0) as f32;
                let x = track.min.x + track.width() * done;
                painter.rect_filled(track, 3.0, Color32::from_white_alpha(70));
                painter.rect_filled(Rect::from_min_max(track.min, pos2(x, track.max.y)), 3.0, t.accent);
                painter.circle_filled(pos2(x, mid), if resp.hovered() || resp.dragged() { 8.0 } else { 6.0 }, Color32::WHITE);
            }
        });
    }

    fn preview(&mut self, ed: &Editor, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        self.refresh_frame(ui.ctx());
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.canvas)).show(ui, |ui| {
            // The picture, and under it what plays it.
            // Room left round the picture: things can be moved off its edges, and found there.
            let whole = ui.max_rect().shrink(36.0);
            let area = Rect::from_min_max(whole.min, pos2(whole.max.x, (whole.max.y - CONTROLS).max(whole.min.y)));
            let mut bottom = area.max.y;
            let mut shown_at = None;
            if let Some(tex) = &self.texture {
                let size = tex.size_vec2();
                // A small video is shown larger than it is: easier to see, and to work on.
                let f = (area.width() / size.x).min(area.height() / size.y).clamp(0.01, PREVIEW_ZOOM);
                let r = Rect::from_center_size(area.center(), size * f);
                ui.painter().image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, t.border), StrokeKind::Outside);
                shown_at = Some(r);
                bottom = r.max.y;
            }
            // The picture is a designer too: what's chosen on it is moved and resized there.
            if let Some(r) = shown_at {
                self.picture_rect = Some(r);
                self.designer(ui, r);
            }
            let now = ui.ctx().input(|i| i.time);
            let playing = self.playing.is_some();
            let length = self.length();
            let (cx, mid) = (area.center().x, bottom + 10.0 + PLAY / 2.0);
            let button = Rect::from_center_size(pos2(cx, mid), vec2(PLAY, PLAY));
            let tip = if playing { "Pause (Space)" } else { "Play (Space)" };
            let resp = ui.interact(button, ui.id().with("play-under-video"), egui::Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, length > 0, tip));
            let fill = if resp.hovered() { t.accent.gamma_multiply(1.15) } else { t.accent };
            ui.painter().circle_filled(button.center(), PLAY / 2.0, fill);
            icons::paint(ui.painter(), button.shrink(PLAY * 0.27), if playing { Icon::Pause } else { Icon::Play }, Color32::WHITE);
            if resp.on_hover_text(tip).clicked() && length > 0 {
                if playing {
                    self.pause();
                } else {
                    self.play(ed, now);
                }
            }
            // Either side of it: a frame back and a frame on, the start and the end.
            let frame = 1_000_000 / self.fps();
            let near = PLAY / 2.0 + 24.0;
            let far = near + 34.0;
            let mut go = None;
            for (dx, icon, tip, to) in [
                (-far, Icon::ToStart, "Go to the Start (Home)", 0),
                (-near, Icon::FrameBack, "Back One Frame (Left Arrow)", self.playhead.saturating_sub(frame)),
                (near, Icon::FrameForward, "Forward One Frame (Right Arrow)", (self.playhead + frame).min(length)),
                (far, Icon::ToEnd, "Go to the End (End)", length),
            ] {
                let r = Rect::from_center_size(pos2(cx + dx, mid), vec2(30.0, 30.0));
                let can = length > 0 && to != self.playhead;
                let resp = ui.interact(r, ui.id().with(("transport", tip)), if can { Sense::click() } else { Sense::hover() });
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, can, tip));
                if resp.hovered() && can {
                    ui.painter().rect_filled(r, 6.0, t.hover);
                }
                let colour = if !can {
                    t.muted.gamma_multiply(0.45)
                } else if resp.hovered() {
                    t.text
                } else {
                    t.muted
                };
                icons::paint(ui.painter(), r.shrink(7.0), icon, colour);
                if resp.on_hover_text(tip).clicked() {
                    go = Some(to);
                }
            }
            if let Some(to) = go {
                self.seek(ed, to, now);
            }
            // The time, after them.
            let words = ui.painter().text(
                pos2(cx + far + 15.0 + 14.0, mid),
                egui::Align2::LEFT_CENTER,
                format!("{} / {}", clock(self.playhead), clock(length)),
                egui::FontId::monospace(14.0),
                t.text,
            );
            // At the picture's right end (clear of the time), the way to the whole screen.
            let right = shown_at.map_or(area.max.x, |r| r.max.x).max(words.max.x + 50.0).min(whole.max.x + 30.0);
            let full = Rect::from_center_size(pos2(right - 15.0, mid), vec2(30.0, 30.0));
            let tip = "Full Screen (F11)";
            let resp = ui.interact(full, ui.id().with("full-screen"), egui::Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
            if resp.hovered() {
                ui.painter().rect_filled(full, 6.0, t.hover);
            }
            icons::paint(ui.painter(), full.shrink(6.0), Icon::FullScreen, if resp.hovered() { t.text } else { t.muted });
            if resp.on_hover_text("Show the video over the whole screen (F11, or F). Esc comes back.").clicked() {
                self.set_full_screen(true, now);
            }
        });
    }

    fn header(&mut self, ed: &mut Editor, ui: &mut Ui, row: Rect, i: usize, track: &Track) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let name = self.rec.stream(track.stream).map(|s| s.kind.label()).unwrap_or("Track");
        ui.painter().rect_filled(Rect::from_min_size(row.min, vec2(4.0, row.height())), 0.0, kind_colour(track.kind));
        let painter = ui.painter_at(row);
        // What it is, drawn and said: faint while it's hidden or muted.
        let icon = match track.kind {
            StreamKind::Screen => Icon::Screen,
            StreamKind::Camera => Icon::Camera,
            StreamKind::SystemSound => Icon::Speaker,
            StreamKind::Microphone => Icon::Microphone,
        };
        let (mark, words) = if track.muted { (t.muted.gamma_multiply(0.7), t.muted) } else { (kind_colour(track.kind), t.text) };
        icons::paint(&painter, Rect::from_min_size(row.min + vec2(12.0, 5.0), vec2(15.0, 15.0)), icon, mark);
        painter.text(row.min + vec2(34.0, 4.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(13.0), words);
        // Every track can be shown or hidden: a hidden picture isn't in the video, a hidden
        // sound isn't heard.
        let shown = !track.muted;
        let switch = Rect::from_min_size(row.min + vec2(9.0, 22.0), vec2(26.0, 20.0));
        if track_switch(ui, ("track", i), switch, if track.kind.is_video() { Switch::Picture } else { Switch::Sound }, shown) {
            self.run(ed, &ctx, "video.mute", json!({"track": i, "muted": shown}));
        }
        if track.kind.is_video() {
            let device = self.rec.stream(track.stream).map(|s| s.name.clone()).unwrap_or_default();
            if !device.is_empty() && device != name {
                painter.text(row.min + vec2(42.0, 25.0), egui::Align2::LEFT_TOP, device, egui::FontId::proportional(11.0), t.muted);
            }
            return;
        }
        let vol = Rect::from_min_size(row.min + vec2(42.0, 22.0), vec2(row.width() - 52.0, 20.0));
        let mut v = track.volume * 100.0;
        let r = volume_slider(ui, vol, &mut v);
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

    /// A layer's header: its name, shown or not, its volume (a sound layer) and a way to delete it.
    fn layer_header(&mut self, ed: &mut Editor, ui: &mut Ui, row: Rect, layer: &screengrabcraft_media::layers::Layer) {
        use screengrabcraft_media::layers::LayerKind;
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let colour = match layer.kind {
            LayerKind::Picture => Color32::from_rgb(0xD9, 0x7B, 0x29),
            LayerKind::Sound => Color32::from_rgb(0x2F, 0x80, 0xED),
        };
        ui.painter().rect_filled(Rect::from_min_size(row.min, vec2(4.0, row.height())), 0.0, colour);
        let painter = ui.painter_at(row);
        let words = if layer.hidden { t.muted } else { t.text };
        painter.text(row.min + vec2(14.0, 4.0), egui::Align2::LEFT_TOP, &layer.name, egui::FontId::proportional(13.0), words);
        let del = Rect::from_min_size(pos2(row.max.x - 26.0, row.min.y + 3.0), vec2(22.0, 18.0));
        let r = ui.put(del, egui::Button::new(RichText::new("×").size(14.0)).frame(false));
        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Delete {}", layer.name)));
        if r.on_hover_text("Delete this layer and what's on it").clicked() {
            if self.item.is_some_and(|(l, _)| l == layer.id) {
                self.item = None;
            }
            self.run(ed, &ctx, "video.deleteLayer", json!({"layer": layer.id}));
            return;
        }
        let shown = !layer.hidden;
        let switch = Rect::from_min_size(row.min + vec2(9.0, 22.0), vec2(26.0, 20.0));
        if track_switch(ui, ("layer", layer.id as usize), switch, if layer.kind == LayerKind::Sound { Switch::Sound } else { Switch::Picture }, shown)
        {
            self.run(ed, &ctx, "video.layer", json!({"layer": layer.id, "hidden": shown}));
        }
        if layer.kind != LayerKind::Sound {
            return;
        }
        let vol = Rect::from_min_size(row.min + vec2(42.0, 22.0), vec2(row.width() - 52.0, 20.0));
        let mut v = layer.volume * 100.0;
        let r = volume_slider(ui, vol, &mut v);
        if r.drag_started() {
            self.session.begin("Volume");
            self.volume_drag = true;
        }
        if r.changed() {
            let p = json!({"layer": layer.id, "volume": v / 100.0});
            if self.volume_drag {
                let _ = self.session.drag("video.layer", &p);
            } else {
                self.run(ed, &ctx, "video.layer", p)
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
            let status = if let Some((_, p)) = self.transcribing() {
                match p {
                    Some(p) => format!("Transcribing on this computer… {:.0}%. The captions appear here when they're ready.", p * 100.0),
                    None => "Transcribing on this computer… the captions appear here when they're ready.".to_string(),
                }
            } else if self.captions.is_some() || !self.media_cues.is_empty() {
                "Nothing said is left in the video: it was cut, muted or hidden where it was said.".to_string()
            } else if self.can_transcribe() {
                "No captions yet. Transcribe writes down what's said in the video, on this computer.".to_string()
            } else {
                "Captions are made of recordings saved in a file.".to_string()
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

    /// The captions' header: while they're being made, how far they've got; then Show (on the
    /// picture) and Again (transcribe again); Transcribe when there are none.
    fn captions_header(&mut self, ed: &mut Editor, ui: &mut Ui, row: Rect) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        ui.painter().rect_filled(Rect::from_min_size(row.min, vec2(4.0, row.height())), 0.0, Color32::from_gray(150));
        let title = Rect::from_min_size(row.min + vec2(14.0, 3.0), vec2(row.width() - 20.0, 18.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(title).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.label(RichText::new("Captions").size(13.0).color(t.text));
        });
        if let Some((what, p)) = self.transcribing() {
            let bar = Rect::from_min_size(row.min + vec2(12.0, 26.0), vec2(row.width() - 24.0, 16.0));
            let words = match p {
                Some(p) => format!("{:.0}%", p * 100.0),
                None => "Waiting…".to_string(),
            };
            ui.put(bar, egui::ProgressBar::new(p.unwrap_or(0.0)).text(words).fill(t.accent).animate(p.is_none())).on_hover_text(what);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        } else if self.captions.is_some() || !self.media_cues.is_empty() {
            let show = self.session.timeline.captions.show;
            let switch = Rect::from_min_size(row.min + vec2(9.0, 22.0), vec2(26.0, 20.0));
            if track_switch(ui, ("captions", 0), switch, Switch::Captions, show) {
                self.run(ed, &ctx, "video.captions", json!({"show": !show}));
            }
            if self.can_transcribe() {
                let b = ui.put(Rect::from_min_size(row.min + vec2(44.0, 22.0), vec2(72.0, 21.0)), egui::Button::new("Again"));
                b.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Transcribe Again"));
                if b.on_hover_text("Transcribe again: what's said in the video, the sound and videos you added included").clicked() {
                    self.transcribe(ed, &ctx, true);
                }
            }
        } else if self.can_transcribe() {
            let b = ui.put(Rect::from_min_size(row.min + vec2(10.0, 23.0), vec2(116.0, 21.0)), egui::Button::new("Transcribe"));
            if b.on_hover_text("Write down what's said in the video, on this computer, in the background").clicked() {
                self.transcribe(ed, &ctx, false);
            }
        }
    }

    fn timeline(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let tracks = self.session.timeline.tracks.clone();
        // The layers above the tracks, the top one first (it's drawn over the others), and a band
        // above them that a drop on makes a layer of its own.
        let layers: Vec<screengrabcraft_media::layers::Layer> = self.session.timeline.layers.iter().rev().cloned().collect();
        let media = self.session.timeline.media.clone();
        let nl = layers.len();
        // The captions' row, under the tracks, when there's sound to have words in.
        let caption_row = self.hears_anything();
        let rows = nl + tracks.len().max(1) + usize::from(caption_row);
        let height = RULER_H + NEW_LAYER_H + rows as f32 * ROW_H + 16.0;
        egui::Panel::bottom("video_timeline")
            .exact_size(height)
            .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(0, 4)))
            .show(ui, |ui| {
                let full = ui.max_rect();
                let lane = Rect::from_min_max(pos2(full.min.x + HEADER_W, full.min.y), full.max);
                let length = self.length().max(1_000_000);
                // Zoom asked for (a button, a key, a menu): around the playhead when it's in view,
                // else around the middle.
                if let Some(f) = self.zoom_ask.take()
                    && self.pps > 0.0
                {
                    let px = lane.min.x + (self.playhead as f64 / 1e6 * f64::from(self.pps)) as f32 - self.scroll;
                    let at = if px >= lane.min.x && px <= lane.max.x { px } else { lane.center().x };
                    let secs = f64::from((at - lane.min.x + self.scroll).max(0.0)) / f64::from(self.pps);
                    self.pps = (self.pps * f).clamp(2.0, 800.0);
                    self.scroll = ((secs * f64::from(self.pps)) as f32 - (at - lane.min.x)).max(0.0);
                }
                if self.pps <= 0.0 {
                    self.pps = ((lane.width() - 24.0) / (length as f32 / 1e6)).clamp(2.0, 400.0);
                    self.scroll = 0.0;
                }
                let (pps, scroll) = (self.pps, self.scroll);
                let x_of = |us: u64| lane.min.x + (us as f64 / 1e6 * f64::from(pps)) as f32 - scroll;
                let us_of = |x: f32| (f64::from((x - lane.min.x + scroll).max(0.0)) / f64::from(pps) * 1e6) as u64;
                let rows_top = full.min.y + RULER_H + NEW_LAYER_H;
                // Row `i` of everything: the layers first, then the tracks.
                let any_row = |i: usize| Rect::from_min_size(pos2(full.min.x, rows_top + i as f32 * ROW_H), vec2(full.width(), ROW_H - 4.0));
                let row_rect = |i: usize| any_row(i + nl);
                let band = Rect::from_min_max(pos2(lane.min.x, full.min.y + RULER_H), pos2(lane.max.x, rows_top - 2.0));
                // The layer under a height, by its id; whether it's the band.
                let layer_at = |y: f32| -> Option<u32> {
                    let r = ((y - rows_top) / ROW_H).floor();
                    (r >= 0.0 && (r as usize) < nl).then(|| layers.get(r as usize).map(|l| l.id)).flatten()
                };
                let in_band = |y: f32| y >= band.min.y && y < rows_top;
                self.lane_rect = Some((lane, rows_top));
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
                // The band for a new layer, lit while something from the library is over it.
                let carrying = egui::DragAndDrop::has_payload_of_type::<LibDrop>(&ctx) || matches!(self.item_drag, Some(ItemDrag::Move { .. }));
                let over_band = resp.hover_pos().or_else(|| ctx.pointer_hover_pos()).is_some_and(|p| band.contains(p));
                let band_fill = if carrying && over_band { t.accent.gamma_multiply(0.35) } else { t.toolbar.gamma_multiply(0.6) };
                painter.rect_filled(band, 3.0, band_fill);
                let band_words =
                    if carrying { "Drop here for a layer of its own" } else { "Layers: drag titles, overlays and media here from the Library" };
                painter.text(band.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, band_words, egui::FontId::proportional(11.0), t.muted);
                // The layers and their items.
                for (k, layer) in layers.iter().enumerate() {
                    let row = any_row(k);
                    let lane_row = Rect::from_min_max(pos2(lane.min.x, row.min.y), pos2(lane.max.x, row.max.y));
                    painter.rect_filled(lane_row, 0.0, if k % 2 == 0 { t.window } else { t.panel });
                    for item in &layer.items {
                        let r = Rect::from_min_max(pos2(x_of(item.at), row.min.y + 3.0), pos2(x_of(item.end()), row.max.y - 3.0));
                        if r.max.x < lane.min.x || r.min.x > lane.max.x {
                            continue;
                        }
                        let colour = if layer.hidden { Color32::from_gray(140) } else { library::item_colour(item, &media) };
                        painter.rect_filled(r, 4.0, colour.gamma_multiply(0.45));
                        painter.rect_stroke(r, 4.0, Stroke::new(1.0, colour), StrokeKind::Inside);
                        painter.with_clip_rect(r.shrink(2.0)).text(
                            r.min + vec2(8.0, 6.0),
                            egui::Align2::LEFT_TOP,
                            item.label(&media),
                            egui::FontId::proportional(12.0),
                            t.text,
                        );
                        if self.item == Some((layer.id, item.id)) {
                            painter.rect_stroke(r, 4.0, Stroke::new(2.5, t.accent), StrokeKind::Outside);
                            grips(&painter, r);
                        }
                    }
                }
                // Something from the library over the timeline: where it would go.
                if let (Some(what), Some(p)) = (resp.dnd_hover_payload::<LibDrop>(), ctx.pointer_hover_pos()) {
                    let len = match what.as_ref() {
                        LibDrop::Template(key) => screengrabcraft_media::layers::template(key).map(|t| t.item.len).unwrap_or(3_000_000),
                        LibDrop::Media(id) => {
                            media.iter().find(|m| m.id == *id).map(|m| if m.duration > 0 { m.duration } else { 5_000_000 }).unwrap_or(3_000_000)
                        }
                    };
                    let y = match layer_at(p.y) {
                        Some(_) => any_row(((p.y - rows_top) / ROW_H).floor().max(0.0) as usize),
                        None => band,
                    };
                    let ghost = Rect::from_min_max(pos2(p.x, y.min.y + 2.0), pos2(x_of(us_of(p.x) + len), y.max.y - 2.0));
                    painter.rect_filled(ghost, 4.0, t.accent.gamma_multiply(0.25));
                    painter.rect_stroke(ghost, 4.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Inside);
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
                            grips(&painter, r);
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
                // Over the ruler, the time under the pointer, and a faint line down from it: where a
                // click would put the playhead. While it's being moved, its own time.
                let scrubbing = matches!(self.drag, Some(Drag::Scrub));
                let over_ruler = resp.hover_pos().filter(|p| ruler.contains(*p) && self.drag.is_none() && self.item_drag.is_none());
                if let Some(x) = over_ruler.map(|p| p.x).or(scrubbing.then_some(px)) {
                    let us = us_of(x).min(self.length());
                    if !scrubbing {
                        painter.line_segment([pos2(x, ruler.max.y), pos2(x, full.max.y)], Stroke::new(1.0, t.muted.gamma_multiply(0.5)));
                    }
                    let words = painter.layout_no_wrap(clock(us), egui::FontId::monospace(11.0), Color32::WHITE);
                    let size = words.size() + vec2(10.0, 4.0);
                    let left = (x - size.x / 2.0).clamp(lane.min.x + 2.0, (lane.max.x - size.x - 2.0).max(lane.min.x + 2.0));
                    let tag = Rect::from_min_size(pos2(left, ruler.min.y + 2.0), size);
                    painter.rect_filled(tag, 4.0, if scrubbing { Color32::from_rgb(0xE5, 0x39, 0x35) } else { Color32::from_black_alpha(210) });
                    painter.galley(tag.min + vec2(5.0, 2.0), words, Color32::WHITE);
                }
                // The headers, over the left of the rows.
                painter.rect_filled(Rect::from_min_max(full.min, pos2(lane.min.x, full.max.y)), 0.0, t.panel);
                painter.line_segment([pos2(lane.min.x, full.min.y), pos2(lane.min.x, full.max.y)], Stroke::new(1.0, t.border));
                painter.text(
                    pos2(full.min.x + 14.0, band.center().y),
                    egui::Align2::LEFT_CENTER,
                    if nl == 0 { "Layers" } else { "Layers (top is in front)" },
                    egui::FontId::proportional(11.0),
                    t.muted,
                );
                // In the corner above them: the timeline's zoom.
                let corner = Rect::from_min_max(pos2(full.min.x + 10.0, full.min.y), pos2(lane.min.x - 4.0, full.min.y + RULER_H));
                ui.scope_builder(egui::UiBuilder::new().max_rect(corner).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                    ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                    if icons::button_sized(ui, Icon::ZoomOut, "Zoom the Timeline Out (Ctrl+-)", false, pps > 2.0, 24.0).clicked() {
                        self.zoom_ask = Some(0.8);
                    }
                    if icons::button_sized(ui, Icon::ZoomIn, "Zoom the Timeline In (Ctrl++)", false, pps < 800.0, 24.0).clicked() {
                        self.zoom_ask = Some(1.25);
                    }
                    if icons::button_sized(ui, Icon::Fit, "Fit the Whole Video on the Timeline (Ctrl+0)", false, true, 24.0).clicked() {
                        self.pps = 0.0;
                    }
                });
                if self.zoom_ask.is_some() {
                    ctx.request_repaint();
                }
                for (k, layer) in layers.iter().enumerate() {
                    let row = any_row(k);
                    self.layer_header(ed, ui, Rect::from_min_max(row.min, pos2(lane.min.x - 1.0, row.max.y)), layer);
                }
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
                    let r = ((y - rows_top) / ROW_H).floor() - nl as f32;
                    (r >= 0.0 && (r as usize) < tracks.len()).then_some(r as usize)
                };
                // Whether `x` is on a block from `a` to `b`: None if not, else which end it's at.
                let on_block = |a: f32, b: f32, x: f32| -> Option<Option<bool>> {
                    if x < a - EDGE || x > b + EDGE {
                        return None;
                    }
                    Some(if (x - a).abs() <= EDGE {
                        Some(true)
                    } else if (x - b).abs() <= EDGE {
                        Some(false)
                    } else {
                        None
                    })
                };
                let clip_at = |track: usize, x: f32| -> Option<(usize, Option<bool>)> {
                    let tr = tracks.get(track)?;
                    tr.clips.iter().enumerate().find_map(|(c, clip)| on_block(x_of(clip.at), x_of(clip.end()), x).map(|e| (c, e)))
                };
                let layer_item_at = |layer: u32, x: f32| -> Option<(u32, Option<bool>)> {
                    let l = layers.iter().find(|l| l.id == layer)?;
                    l.items.iter().find_map(|it| on_block(x_of(it.at), x_of(it.end()), x).map(|e| (it.id, e)))
                };
                if let Some(p) = resp.hover_pos() {
                    let on_edge = match layer_at(p.y) {
                        Some(l) => matches!(layer_item_at(l, p.x), Some((_, Some(_)))),
                        None => row_at(p.y).is_some_and(|track| matches!(clip_at(track, p.x), Some((_, Some(_))))),
                    };
                    if on_edge {
                        ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    }
                    // What's under the pointer says what it is and how long (after a moment).
                    if self.drag.is_none() && self.item_drag.is_none() && p.y >= ruler.max.y {
                        let about = match layer_at(p.y) {
                            Some(l) => layer_item_at(l, p.x)
                                .and_then(|(id, _)| layers.iter().find(|x| x.id == l).and_then(|x| x.items.iter().find(|i| i.id == id)))
                                .map(|it| (it.label(&media), it.at, it.end())),
                            None => row_at(p.y).and_then(|track| {
                                let (c, _) = clip_at(track, p.x)?;
                                let tr = tracks.get(track)?;
                                let clip = tr.clips.get(c)?;
                                Some((tr.kind.label().to_string(), clip.at, clip.end()))
                            }),
                        };
                        if let Some((name, from, to)) = about {
                            let tip = format!(
                                "{name}: {} to {} ({} long)\nDrag it to slide it, or one of its ends to trim it.",
                                clock(from),
                                clock(to),
                                clock(to.saturating_sub(from))
                            );
                            resp.clone().on_hover_text_at_pointer(tip);
                        }
                    }
                }
                // Edges catch what's dragged along the timeline when it comes within a few points
                // of them: the playhead, the video's ends, the ends of every clip and item but
                // the one dragged. Alt held, nothing catches.
                let (head, whole) = (self.playhead, self.length());
                let no_catch = ctx.input(|i| i.modifiers.alt);
                let reach = (f64::from(SNAP) / f64::from(pps.max(0.01)) * 1e6) as u64;
                let edges = |but_clip: Option<(usize, usize)>, but_item: Option<(u32, u32)>| -> Vec<u64> {
                    let mut e = vec![0, head, whole];
                    for (ti, tr) in tracks.iter().enumerate() {
                        for (ci, c) in tr.clips.iter().enumerate() {
                            if but_clip != Some((ti, ci)) {
                                e.push(c.at);
                                e.push(c.end());
                            }
                        }
                    }
                    for l in &layers {
                        for it in &l.items {
                            if but_item != Some((l.id, it.id)) {
                                e.push(it.at);
                                e.push(it.end());
                            }
                        }
                    }
                    e
                };
                let nearest = |us: u64, edges: &[u64]| -> Option<u64> {
                    if no_catch {
                        return None;
                    }
                    edges.iter().copied().filter(|e| e.abs_diff(us) <= reach).min_by_key(|e| e.abs_diff(us))
                };
                // Something `len` long put at `at`: where it goes when its start or its end is
                // caught (the nearer), and the edge that caught it.
                let caught_at = |at: u64, len: u64, edges: &[u64]| -> (u64, Option<u64>) {
                    match (nearest(at, edges), nearest(at.saturating_add(len), edges)) {
                        (Some(a), Some(b)) if a.abs_diff(at) <= b.abs_diff(at.saturating_add(len)) => (a, Some(a)),
                        (_, Some(b)) => (b.saturating_sub(len), Some(b)),
                        (Some(a), None) => (a, Some(a)),
                        (None, None) => (at, None),
                    }
                };
                let mut caught: Option<u64> = None;
                // Something from the library let go of here: on the layer under it (one of its
                // kind, where it fits), else on a layer of its own.
                if let Some(what) = resp.dnd_release_payload::<LibDrop>()
                    && let Some(p) = ctx.pointer_interact_pos().or_else(|| ctx.pointer_hover_pos())
                {
                    let layer = if in_band(p.y) { None } else { layer_at(p.y) };
                    self.drop_on_timeline(ed, &ctx, &what, layer, us_of(p.x));
                }
                // A layer's item taken hold of: moved, or trimmed by an end.
                let mut item_grabbed = false;
                if resp.drag_started()
                    && let Some(p) = ctx.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos())
                    && let Some(layer) = layer_at(p.y)
                    && let Some((item, edge)) = layer_item_at(layer, p.x)
                {
                    item_grabbed = true;
                    self.item = Some((layer, item));
                    self.selected = None;
                    self.camera_chosen = false;
                    self.item_drag = Some(match edge {
                        Some(start) => {
                            self.session.begin("Trim");
                            ItemDrag::Trim { layer, item, start }
                        }
                        None => {
                            self.session.begin("Move");
                            let at = self.session.timeline.item(layer, item).map(|i| i.at).unwrap_or(0);
                            ItemDrag::Move { layer, item, grab: us_of(p.x).saturating_sub(at), from: layer }
                        }
                    });
                }
                if resp.dragged()
                    && let (Some(p), Some(d)) = (resp.interact_pointer_pos(), self.item_drag)
                {
                    let us = us_of(p.x);
                    match d {
                        ItemDrag::Trim { layer, item, start } => {
                            if let Some(it) = self.session.timeline.item(layer, item).cloned() {
                                caught = nearest(us, &edges(None, Some((layer, item))));
                                let us = caught.unwrap_or(us);
                                let (a, b) = if start { (us, it.end()) } else { (it.at, us) };
                                let _ = self.session.drag("video.trimItem", &json!({"layer": layer, "item": item, "start": a, "end": b}));
                            }
                        }
                        ItemDrag::Move { layer, item, grab, from } => {
                            // Onto the layer under the pointer when it's of the item's kind.
                            let kind = self.session.timeline.layer(layer).map(|l| l.kind);
                            let to = layer_at(p.y).filter(|id| self.session.timeline.layer(*id).map(|l| l.kind) == kind).unwrap_or(layer);
                            let len = self.session.timeline.item(layer, item).map_or(0, |it| it.end().saturating_sub(it.at));
                            let (at, edge) = caught_at(us.saturating_sub(grab), len, &edges(None, Some((layer, item))));
                            caught = edge;
                            let params = json!({"layer": layer, "item": item, "toLayer": to, "at": at});
                            if let Ok(v) = self.session.drag("video.moveItem", &params) {
                                let now_on = v.get("layer").and_then(Value::as_u64).map_or(layer, |l| l as u32);
                                self.item_drag = Some(ItemDrag::Move { layer: now_on, item, grab, from });
                                self.item = Some((now_on, item));
                            }
                        }
                    }
                    if p.x > lane.max.x - 20.0 {
                        self.scroll += 8.0;
                    } else if p.x < lane.min.x + 20.0 {
                        self.scroll = (self.scroll - 8.0).max(0.0);
                    }
                }
                if resp.drag_stopped()
                    && let Some(d) = self.item_drag.take()
                {
                    if let ItemDrag::Move { layer, item, grab, from } = d {
                        // Let go on the band: a layer of its own.
                        if let Some(p) = ctx.pointer_interact_pos().or_else(|| ctx.pointer_hover_pos())
                            && in_band(p.y)
                            && let Ok(v) = self.session.drag(
                                "video.moveItem",
                                &json!({"layer": layer, "item": item, "toLayer": "new", "at": us_of(p.x).saturating_sub(grab)}),
                            )
                        {
                            let now_on = v.get("layer").and_then(Value::as_u64).map_or(layer, |l| l as u32);
                            self.item = Some((now_on, item));
                        }
                        // The layer it came from, left empty by it, goes.
                        let now_on = self.item.map_or(layer, |(l, _)| l);
                        if from != now_on && self.session.timeline.layer(from).is_some_and(|l| l.items.is_empty()) {
                            let _ = self.session.drag("video.deleteLayer", &json!({"layer": from}));
                        }
                    }
                    self.session.commit();
                }
                if !item_grabbed
                    && resp.drag_started()
                    && let Some(p) = ctx.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos())
                {
                    self.item_drag = None;
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
                            caught = nearest(us, &edges(Some((track, clip)), None));
                            let _ = self.session.drag(id, &json!({"track": track, "clip": clip, "to": caught.unwrap_or(us)}));
                        }
                        Drag::Move { track, clip, grab } => {
                            let len = tracks.get(track).and_then(|tr| tr.clips.get(clip)).map_or(0, |c| c.len);
                            let (to, edge) = caught_at(us.saturating_sub(grab), len, &edges(Some((track, clip)), None));
                            caught = edge;
                            let _ = self.session.drag("video.moveClip", &json!({"track": track, "clip": clip, "to": to}));
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
                // The edge that caught what's being dragged, shown down the timeline.
                if let Some(edge) = caught {
                    let x = x_of(edge);
                    if x >= lane.min.x && x <= lane.max.x {
                        painter.line_segment([pos2(x, ruler.max.y), pos2(x, full.max.y)], Stroke::new(1.5, Color32::from_rgb(0xFF, 0xD5, 0x4F)));
                    }
                }
                if resp.clicked()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    let on_item = layer_at(p.y).and_then(|l| layer_item_at(l, p.x).map(|(i, _)| (l, i)));
                    match row_at(p.y).and_then(|track| clip_at(track, p.x).map(|(c, _)| (track, c))) {
                        _ if on_item.is_some() => {
                            self.item = on_item;
                            self.selected = None;
                            self.camera_chosen = false;
                        }
                        Some(sel) if p.y >= ruler.max.y => {
                            self.selected = Some(sel);
                            self.item = None;
                        }
                        _ => {
                            self.selected = None;
                            self.item = None;
                            self.range = None;
                            self.seek(ed, us_of(p.x), now);
                        }
                    }
                }
                // The right button: what's under it is chosen, and a menu is about it and that moment.
                if resp.secondary_clicked()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    let on_item = layer_at(p.y).and_then(|l| layer_item_at(l, p.x).map(|(i, _)| (l, i)));
                    let on_clip = row_at(p.y).and_then(|track| clip_at(track, p.x).map(|(c, _)| (track, c))).filter(|_| p.y >= ruler.max.y);
                    if on_item.is_some() {
                        self.item = on_item;
                        self.selected = None;
                        self.camera_chosen = false;
                    } else if on_clip.is_some() {
                        self.selected = on_clip;
                        self.item = None;
                    } else {
                        // On nothing: its menu is about that moment, for every track.
                        self.selected = None;
                        self.item = None;
                    }
                    self.menu_us = Some(us_of(p.x).min(self.length()));
                }
                resp.context_menu(|ui| {
                    let at = self.menu_us.unwrap_or(self.playhead);
                    if ui.button(format!("Split Here ({})", clock(at))).clicked() {
                        self.seek(ed, at, now);
                        self.split(ed, &ctx);
                        ui.close();
                    }
                    if ui.button("Move the Playhead Here").clicked() {
                        self.seek(ed, at, now);
                        ui.close();
                    }
                    let any = self.item.is_some() || self.range.is_some() || self.selected.is_some();
                    let label = if self.item.is_some() {
                        "Delete the Item"
                    } else if self.range.is_some() {
                        "Cut the Chosen Stretch"
                    } else {
                        "Delete the Clip"
                    };
                    if ui.add_enabled(any, egui::Button::new(label).shortcut_text("Del")).clicked() {
                        self.delete(ed, &ctx);
                        ui.close();
                    }
                    ui.separator();
                    if ui.add(egui::Button::new("Zoom In").shortcut_text("Ctrl++")).clicked() {
                        self.zoom_ask = Some(1.25);
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Zoom Out").shortcut_text("Ctrl+-")).clicked() {
                        self.zoom_ask = Some(0.8);
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Fit the Whole Video").shortcut_text("Ctrl+0")).clicked() {
                        self.pps = 0.0;
                        ui.close();
                    }
                });
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
                // A bar along the bottom to scroll by, when the video is longer than the timeline
                // shows: as long as the share in view. (A little room is left past the end.)
                let content = (self.length() as f64 / 1e6 * f64::from(self.pps)) as f32 + 24.0;
                let all = content + lane.width() * 0.25;
                let furthest = if content > lane.width() + 1.0 { all - lane.width() } else { 0.0 };
                if furthest > 1.0 {
                    let track = Rect::from_min_max(pos2(lane.min.x + 4.0, full.max.y - 7.0), pos2(lane.max.x - 4.0, full.max.y - 1.0));
                    let bar = ui.interact(track.expand2(vec2(0.0, 3.0)), ui.id().with("timeline-scroll"), Sense::click_and_drag());
                    bar.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, "Scroll the Timeline"));
                    let share = (lane.width() / all).clamp(0.05, 1.0);
                    let thumb_w = (track.width() * share).max(28.0);
                    let run = (track.width() - thumb_w).max(1.0);
                    if bar.dragged() {
                        self.scroll += bar.drag_delta().x / run * furthest;
                    } else if bar.clicked()
                        && let Some(p) = bar.interact_pointer_pos()
                    {
                        self.scroll = ((p.x - track.min.x - thumb_w / 2.0) / run).clamp(0.0, 1.0) * furthest;
                    }
                    self.scroll = self.scroll.clamp(0.0, furthest);
                    let x = track.min.x + self.scroll / furthest * run;
                    let thumb = Rect::from_min_size(pos2(x, track.min.y), vec2(thumb_w, track.height()));
                    painter.rect_filled(track, 3.0, t.window);
                    painter.rect_filled(thumb, 3.0, if bar.hovered() || bar.dragged() { t.muted } else { t.control_border });
                } else if self.item_drag.is_none() && self.drag.is_none() {
                    // All of it fits: nothing to scroll to.
                    self.scroll = self.scroll.min(furthest);
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
    if v.full_screen {
        // Nothing but the picture and its controls.
        v.full_preview(ed, ui);
        if ed.video_epoch == epoch && ed.video.is_none() {
            ed.video = Some(v);
        } else {
            v.save_now(ed, &ctx);
        }
        return;
    }
    v.menu_bar(ed, ui);
    v.tool_bar(ed, ui);
    v.status_bar(ui, ed.transcripts.status());
    if ed.settings.show_tray {
        crate::strip::show(ed, ui);
    }
    v.timeline(ed, ui);
    if !v.item_panel(ed, ui) {
        v.camera_panel(ed, ui);
    }
    v.library_panel(ed, ui);
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
