//! Several recordings made into videos at once, with one setting for all of them (File › Make
//! Videos of Several Recordings): each recording read, the media its layers use read, and its
//! video made beside it (its own name, the format's extension) as it was edited, one after
//! another; the window says how each is getting on.

use std::sync::{Arc, Mutex};

use egui::RichText;
use screengrabcraft_media::media_store::{MediaJob, MediaLibrary};
use screengrabcraft_media::recording::Recording;
use screengrabcraft_media::timeline::Timeline;

use super::VideoView;
use crate::editor::Editor;
use crate::services::{VideoExport, VideoFormat, VideoJob};
use crate::theme::Tokens;
use crate::widgets::{self, FooterButton, Role};

/// How one recording is getting on.
#[derive(Clone, Debug, PartialEq)]
pub enum BatchState {
    Waiting,
    /// Being read, or its media, or its video being made (how far, 0 to 1).
    Working(String, f32),
    /// Made: where.
    Done(String),
    Failed(String),
}

pub struct BatchItem {
    pub path: String,
    pub state: BatchState,
}

/// What's being done to the recording under way.
enum Step {
    Opening(Arc<Mutex<Option<Result<Recording, String>>>>),
    Media { rec: Arc<Recording>, timeline: Box<Timeline>, library: Arc<MediaLibrary>, jobs: Vec<(u32, MediaJob)> },
    Making(VideoExport),
}

/// Several recordings to make videos of, and how.
pub struct Batch {
    pub items: Vec<BatchItem>,
    pub format: VideoFormat,
    /// What can be made here.
    pub formats: Vec<VideoFormat>,
    /// The captions burned into the pictures (else written beside each video, when it has sound).
    pub burn: bool,
    pub started: bool,
    running: Option<(usize, Step)>,
}

/// The most recordings made at once.
pub const MAX_BATCH: usize = 200;

/// `path` with its extension changed to `ext`.
fn with_extension(path: &str, ext: &str) -> String {
    std::path::Path::new(path).with_extension(ext).to_string_lossy().to_string()
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

impl Batch {
    /// The recordings at `paths` (video recordings only, each once), to be made as `formats`'s
    /// first kind with pictures.
    pub fn new(paths: &[String], formats: Vec<VideoFormat>) -> Batch {
        let mut items: Vec<BatchItem> = Vec::new();
        for p in paths.iter().filter(|p| p.to_ascii_lowercase().ends_with(".sgvideo")).take(MAX_BATCH) {
            if !items.iter().any(|i| i.path == *p) {
                items.push(BatchItem { path: p.clone(), state: BatchState::Waiting });
            }
        }
        let format = formats.iter().copied().find(|f| f.has_picture()).or(formats.first().copied()).unwrap_or(VideoFormat::Mp4Av1);
        Batch { items, format, formats, burn: false, started: false, running: None }
    }

    /// Whether one is being made now.
    pub fn busy(&self) -> bool {
        self.running.is_some()
    }

    /// Stops: the video being made is let go (nothing of it is left), the rest wait.
    pub fn stop(&mut self) {
        if let Some((k, step)) = self.running.take() {
            if let Step::Making(x) = &step {
                x.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            if let Some(i) = self.items.get_mut(k) {
                i.state = BatchState::Waiting;
            }
        }
        self.started = false;
    }

    /// Moves the one under way on, or starts the next.
    pub fn poll(&mut self, ed: &mut Editor) {
        if !self.started {
            return;
        }
        let Some((k, step)) = self.running.take() else {
            match self.items.iter().position(|i| i.state == BatchState::Waiting) {
                Some(k) => self.open(ed, k),
                None => self.started = false,
            }
            return;
        };
        let path = self.items.get(k).map(|i| i.path.clone()).unwrap_or_default();
        let next = match step {
            Step::Opening(done) => match done.lock().ok().and_then(|mut d| d.take()) {
                None => Some(Step::Opening(done)),
                Some(Err(e)) => self.failed(k, e),
                Some(Ok(rec)) => {
                    // Its own edits (or as recorded), and the media they use read where it is.
                    let rec = Arc::new(rec);
                    let timeline = rec.timeline.clone().map(|t| t.checked(&rec)).unwrap_or_else(|| Timeline::new(&rec));
                    let library = Arc::new(MediaLibrary::default());
                    let mut jobs = Vec::new();
                    for m in timeline.media.iter().take(screengrabcraft_media::layers::MAX_MEDIA) {
                        library.loading(m.id);
                        match ed.services.load_media(&m.path) {
                            Ok(job) => jobs.push((m.id, job)),
                            Err(e) => library.failed(m.id, e),
                        }
                    }
                    self.working(k, "Reading its media…", 0.0);
                    Some(Step::Media { rec, timeline: Box::new(timeline), library, jobs })
                }
            },
            Step::Media { rec, timeline, library, mut jobs } => {
                jobs.retain(|(id, job)| match job.lock().ok().and_then(|mut d| d.take()) {
                    Some(Ok(loaded)) => {
                        library.insert(*id, loaded.content);
                        false
                    }
                    Some(Err(e)) => {
                        library.failed(*id, e);
                        false
                    }
                    None => true,
                });
                if !jobs.is_empty() { Some(Step::Media { rec, timeline, library, jobs }) } else { self.make(ed, k, &path, rec, *timeline, library) }
            }
            Step::Making(x) => match x.done.lock().ok().and_then(|mut d| d.take()) {
                None => {
                    let p = x.progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
                    self.working(k, "Making its video…", p);
                    Some(Step::Making(x))
                }
                Some(Ok(made)) => {
                    if let Some(i) = self.items.get_mut(k) {
                        i.state = BatchState::Done(made);
                    }
                    None
                }
                Some(Err(e)) => self.failed(k, e),
            },
        };
        self.running = next.map(|s| (k, s));
    }

    fn working(&mut self, k: usize, what: &str, p: f32) {
        if let Some(i) = self.items.get_mut(k) {
            i.state = BatchState::Working(what.to_string(), p);
        }
    }

    fn failed(&mut self, k: usize, why: String) -> Option<Step> {
        if let Some(i) = self.items.get_mut(k) {
            i.state = BatchState::Failed(why);
        }
        None
    }

    /// Starts reading recording `k` (on a thread of its own, where there are threads).
    fn open(&mut self, ed: &mut Editor, k: usize) {
        let Some(path) = self.items.get(k).map(|i| i.path.clone()) else { return };
        let source = match ed.services.open_recording(&path) {
            Ok(s) => s,
            Err(e) => {
                self.failed(k, e);
                return;
            }
        };
        let done: Arc<Mutex<Option<Result<Recording, String>>>> = Arc::default();
        let read = {
            let done = done.clone();
            move || {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Recording::open(source).map_err(|e| e.to_string())))
                    .unwrap_or_else(|_| Err("the recording couldn't be read".to_string()));
                if let Ok(mut d) = done.lock() {
                    *d = Some(r);
                }
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Err(e) = std::thread::Builder::new().name("batch-recording".into()).spawn(read) {
            self.failed(k, e.to_string());
            return;
        }
        #[cfg(target_arch = "wasm32")]
        read();
        self.working(k, "Reading it…", 0.0);
        self.running = Some((k, Step::Opening(done)));
    }

    /// Starts making recording `k`'s video, beside it.
    fn make(&mut self, ed: &mut Editor, k: usize, path: &str, rec: Arc<Recording>, timeline: Timeline, library: Arc<MediaLibrary>) -> Option<Step> {
        if timeline.length() == 0 {
            return self.failed(k, "there's nothing left of it to make a video of".to_string());
        }
        let said = screengrabcraft_media::captions::as_corrected(rec.captions.as_ref(), &timeline);
        let cues = screengrabcraft_media::captions::edited_cues(said.as_ref(), &timeline, &Default::default());
        let captions = (!cues.is_empty()).then_some(cues);
        let burn = captions.is_some() && self.burn && self.format.has_picture();
        let job = VideoJob {
            recording: rec.clone(),
            timeline,
            out: with_extension(path, self.format.extension()),
            replace: false,
            format: self.format,
            max_size: (1920, 1080),
            fps: rec.header.fps.clamp(5, 60),
            subtitles: captions.is_some() && !burn && self.format.has_sound(),
            burn,
            captions,
            media: library,
        };
        match ed.services.export_video(job) {
            Ok(x) => {
                self.working(k, "Making its video…", 0.0);
                Some(Step::Making(x))
            }
            Err(e) => self.failed(k, e),
        }
    }
}

impl VideoView {
    /// Opens the window for making videos of the recordings at `paths`.
    pub fn open_batch(&mut self, ed: &mut Editor, ctx: &egui::Context, paths: &[String]) {
        let b = Batch::new(paths, ed.services.video_formats());
        if b.items.is_empty() {
            ed.error(ctx, "Choose video recordings (.sgvideo) to make videos of.".to_string());
            return;
        }
        self.batch = Some(b);
    }

    /// The batch's window: the recordings and how each is getting on, the one setting, and Make
    /// Them, Stop or Close.
    pub(super) fn batch_window(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let Some(mut b) = self.batch.take() else { return };
        b.poll(ed);
        if b.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        let t = Tokens::get(ctx);
        let frame = widgets::window_frame(&ctx.global_style());
        let mut answer = None;
        egui::Modal::new(egui::Id::new("batch-videos")).frame(frame).show(ctx, |ui| {
            ui.set_width(540.0);
            let closed = widgets::title_bar(ui, "Make Several Videos");
            widgets::content(ui, |ui| {
                ui.label(RichText::new("Each is made as it was edited, beside its recording, with its own name.").small().color(t.muted));
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    for i in &b.items {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(file_name(&i.path)).color(t.text));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match &i.state {
                                BatchState::Waiting => {
                                    ui.label(RichText::new("Waiting").small().color(t.muted));
                                }
                                BatchState::Working(what, p) => {
                                    ui.add(egui::ProgressBar::new(*p).desired_width(140.0).text(what.as_str()));
                                }
                                BatchState::Done(made) => {
                                    ui.label(RichText::new("Made").small().color(t.accent)).on_hover_text(made.as_str());
                                }
                                BatchState::Failed(why) => {
                                    ui.label(RichText::new("Not made").small().color(t.danger)).on_hover_text(why.as_str());
                                }
                            });
                        });
                    }
                });
                ui.add_space(10.0);
                ui.add_enabled_ui(!b.started, |ui| {
                    ui.label(RichText::new("Format").strong().color(t.text));
                    egui::ComboBox::from_id_salt("batch-format").width(ui.available_width()).selected_text(b.format.label()).show_ui(ui, |ui| {
                        for f in VideoFormat::ALL {
                            let can = b.formats.contains(&f);
                            ui.add_enabled_ui(can, |ui| ui.selectable_value(&mut b.format, f, f.label())).inner.on_disabled_hover_text(f.why_not());
                        }
                    });
                    ui.add_space(6.0);
                    ui.add_enabled_ui(b.format.has_picture(), |ui| {
                        ui.checkbox(&mut b.burn, "Captions in the picture")
                            .on_hover_text("Else, with sound, written beside each video as subtitles (.srt), where there are some");
                    });
                });
            });
            let start = if b.started { FooterButton::primary("Stop") } else { FooterButton::primary("Make Them") };
            let buttons = [start, FooterButton::cancel("Close")];
            answer = widgets::dialog_footer(ui, &buttons).and_then(|i| buttons.get(i)).map(|x| x.role);
            if closed {
                answer = Some(Role::Cancel);
            }
        });
        match answer {
            Some(Role::Default) if b.started => b.stop(),
            Some(Role::Default) => {
                // Again for those not made, the others kept.
                for i in &mut b.items {
                    if matches!(i.state, BatchState::Failed(_)) {
                        i.state = BatchState::Waiting;
                    }
                }
                b.started = true;
            }
            Some(_) => {
                b.stop();
                return;
            }
            None => {}
        }
        self.batch = Some(b);
    }
}
