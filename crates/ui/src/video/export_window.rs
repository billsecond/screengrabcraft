//! Export Video: a window to choose where the video goes, what it's made as (a video, a GIF, or
//! just the sound), whether the whole of it or just the chosen stretch, whether its captions
//! go with it, and a watermark on it; a window while it's being made, with Cancel; and one when
//! it's made, to play it or show it selected in its folder. And Save the Frame as a Picture: the
//! picture at the playhead, opened in the Image Editor.

use egui::RichText;

use super::VideoView;
use crate::editor::{Editor, file_name};
use crate::services::{VideoExport, VideoFormat, VideoJob};
use crate::theme::Tokens;
use crate::widgets::{self, FooterButton, Role};

/// What becomes of the captions in a video made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionsIn {
    /// Left out.
    Off,
    /// Burned into the pictures: always shown, in any player.
    Picture,
    /// A subtitles file beside the video (`.srt`, its name): players show them when asked.
    Subtitles,
}

impl CaptionsIn {
    fn label(self) -> &'static str {
        match self {
            CaptionsIn::Off => "Don't include them",
            CaptionsIn::Picture => "On the picture (always shown)",
            CaptionsIn::Subtitles => "As a subtitles file beside it (.srt)",
        }
    }
}

/// What the Export Video window is set to.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportChoice {
    /// Where it goes.
    pub path: String,
    /// Picked in the save dialog (a file there is replaced); else a free name beside it is used.
    pub chosen: bool,
    pub format: VideoFormat,
    pub captions: CaptionsIn,
    /// What can be made here, the one every player opens first.
    pub formats: Vec<VideoFormat>,
    /// The stretch chosen on the timeline when the window opened, if one was: it can be made
    /// alone.
    pub stretch: Option<(u64, u64)>,
    /// Just that stretch is made (else the whole video).
    pub only_stretch: bool,
}

/// The captions' choice made to fit what's made: no subtitles beside a GIF (it has no sound),
/// none on the picture of just the sound.
fn fit_captions(c: &mut ExportChoice) {
    if !c.format.has_sound() && c.captions == CaptionsIn::Subtitles {
        c.captions = CaptionsIn::Picture;
    }
    if !c.format.has_picture() && c.captions == CaptionsIn::Picture {
        c.captions = CaptionsIn::Subtitles;
    }
}

/// What a file made is, by its name: words for its windows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Made {
    Video,
    Gif,
    Sound,
}

impl Made {
    fn of(path: &str) -> Made {
        let p = path.to_ascii_lowercase();
        if p.ends_with(".gif") {
            Made::Gif
        } else if p.ends_with(".wav") || p.ends_with(".m4a") {
            Made::Sound
        } else {
            Made::Video
        }
    }
}

/// Export Video's windows, one at a time.
pub enum ExportStep {
    Choose(ExportChoice),
    /// Being made, into `path`, with its subtitles beside it if `subtitles`.
    Making {
        job: VideoExport,
        path: String,
        subtitles: bool,
    },
    /// Made: the file, and its subtitles if they were written beside it.
    Made {
        path: String,
        subtitles: Option<String>,
    },
}

/// `path` with another extension.
fn with_extension(path: &str, ext: &str) -> String {
    std::path::Path::new(path).with_extension(ext).to_string_lossy().to_string()
}

/// The folder a file is in, for saying where it went.
fn folder(path: &str) -> String {
    std::path::Path::new(path).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

impl VideoView {
    /// Export Video: opens the window to choose how (nothing is made until Export).
    pub fn open_export(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        self.open_export_as(ed, ctx, false);
    }

    /// Export Video's window, set to make just the sound (`sound`), or as it was last set.
    pub fn open_export_as(&mut self, ed: &mut Editor, ctx: &egui::Context, sound: bool) {
        if matches!(self.export, Some(ExportStep::Making { .. })) {
            return;
        }
        let Some(recording) = self.path.clone() else {
            ed.error(ctx, "Only a recording opened from a file can be made into a video here");
            return;
        };
        let formats = ed.services.video_formats();
        let Some(first) = formats.first().copied() else {
            ed.error(ctx, "Videos are made by ScreenGrabCraft Editor on the desktop");
            return;
        };
        let have_captions = !self.cues.is_empty();
        let usual = if self.session.timeline.captions.burn { CaptionsIn::Picture } else { CaptionsIn::Subtitles };
        let (format, captions) = match self.export_last {
            Some((f, c)) if formats.contains(&f) && f.has_picture() != sound => (f, c),
            _ if sound => match formats.iter().copied().find(|f| !f.has_picture()) {
                Some(f) => (f, CaptionsIn::Off),
                None => {
                    ed.error(ctx, "Sound files are made by ScreenGrabCraft Editor on the desktop");
                    return;
                }
            },
            _ => (formats.iter().copied().find(|f| f.has_picture()).unwrap_or(first), usual),
        };
        let path = with_extension(&recording, format.extension());
        let stretch = self.range.filter(|(a, b)| b > a);
        let mut c = ExportChoice { path, chosen: false, format, captions, formats, stretch, only_stretch: false };
        if !have_captions {
            c.captions = CaptionsIn::Off;
        }
        fit_captions(&mut c);
        self.export = Some(ExportStep::Choose(c));
    }

    /// The picture at the playhead (as the video shows it, at the recording's own size), saved
    /// in the captures folder and opened in the Image Editor: its window, through the host (in a
    /// browser, downloaded).
    pub fn save_frame(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        use screengrabcraft_model::ImageFormat;
        let (sw, sh) = self
            .rec
            .header
            .streams
            .iter()
            .find(|s| s.kind == screengrabcraft_media::recording::StreamKind::Screen)
            .or_else(|| self.rec.header.streams.iter().find(|s| s.kind == screengrabcraft_media::recording::StreamKind::Camera))
            .map(|s| (s.width, s.height))
            .unwrap_or((1280, 720));
        let (w, h) = screengrabcraft_media::compose::fit(sw, sh, 7680, 4320);
        let timeline = &self.session.timeline;
        let mut composer = screengrabcraft_media::compose::Composer::new(self.rec.clone(), timeline, w, h);
        composer.set_media(self.library.clone());
        composer.set_captions(self.showing_captions().then(|| self.cues.clone()));
        let at = self.playhead.min(self.length().saturating_sub(1));
        let picture = composer.frame(timeline, at);
        let stem = std::path::Path::new(&self.name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Video".to_string());
        let source = format!("{stem} {}", super::clock(at).replace(':', "."));
        if !ed.services.has_paths() {
            let made = screengrabcraft_render::encode(&picture, ImageFormat::Png, 90).map_err(|e| e.to_string());
            match made.and_then(|png| ed.services.download(&format!("{source}.png"), &png)) {
                Ok(()) => ed.toast(ctx, "The frame was saved as a picture"),
                Err(e) => ed.error(ctx, format!("The frame could not be saved: {e}")),
            }
            return;
        }
        let saved = ed.services.new_picture_path(&source).and_then(|path| {
            let format = std::path::Path::new(&path).extension().and_then(|e| e.to_str()).and_then(ImageFormat::from_extension);
            let format = match format {
                Some(f @ (ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Webp | ImageFormat::Bmp)) => f,
                _ => ImageFormat::Png,
            };
            let bytes = screengrabcraft_render::encode(&picture, format, 92).map_err(|e| e.to_string())?;
            ed.services.write_file(&path, &bytes)?;
            Ok(path)
        });
        match saved {
            Ok(path) => {
                log::info!("a frame saved as a picture: {w} × {h}, {}", file_name(&path));
                if ed.services.open_in_other_editor(&path, false) {
                    ed.toast(ctx, format!("{} saved, and opened in the Image Editor", file_name(&path)));
                } else {
                    ed.toast(ctx, format!("{} saved in the captures folder", file_name(&path)));
                }
            }
            Err(e) => ed.error(ctx, format!("The frame could not be saved: {e}")),
        }
    }

    /// The save dialog Export Video's Change… opened has answered: the video goes there. A name
    /// ending as another kind of file this computer makes (`.gif`, `.wav`) makes that kind.
    pub fn export_saved_to(&mut self, path: &str) {
        let Some(ExportStep::Choose(c)) = self.export.as_mut() else { return };
        let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        if let Some(ext) = ext.as_deref()
            && ext != c.format.extension()
            && let Some(f) = c.formats.iter().copied().find(|f| f.extension() == ext)
        {
            c.format = f;
        }
        fit_captions(c);
        // A name without the extension of what's made gets it.
        c.path = if ext.as_deref() == Some(c.format.extension()) { path.to_string() } else { format!("{path}.{}", c.format.extension()) };
        c.chosen = true;
    }

    /// What's made from the edits as chosen: the whole video, or a copy cut to the stretch.
    fn export_timeline(&self, c: &ExportChoice) -> Result<screengrabcraft_media::timeline::Timeline, String> {
        match c.stretch.filter(|_| c.only_stretch) {
            Some((a, b)) => self.session.timeline.stretch(a, b).map_err(|e| e.to_string()),
            None => Ok(self.session.timeline.clone()),
        }
    }

    /// Starts making the video as chosen.
    fn start_export(&mut self, ed: &mut Editor, ctx: &egui::Context, c: &ExportChoice) {
        let timeline = match self.export_timeline(c) {
            Ok(t) => t,
            Err(e) => {
                self.export = None;
                ed.error(ctx, format!("The stretch can't be made: {e}"));
                return;
            }
        };
        // The captions as they fall on what's made (just the stretch: from its start).
        let cues = if c.only_stretch && c.stretch.is_some() {
            let said = screengrabcraft_media::captions::as_corrected(self.captions.as_ref(), &timeline);
            screengrabcraft_media::captions::edited_cues(said.as_ref(), &timeline, &self.media_cues)
        } else {
            self.cues.to_vec()
        };
        let captions = (!cues.is_empty() && c.captions != CaptionsIn::Off).then_some(cues);
        let subtitles = captions.is_some() && c.captions == CaptionsIn::Subtitles && c.format.has_sound();
        let job = VideoJob {
            recording: self.rec.clone(),
            timeline,
            out: c.path.clone(),
            replace: c.chosen,
            format: c.format,
            max_size: (1920, 1080),
            fps: self.rec.header.fps.clamp(5, 60),
            burn: captions.is_some() && c.captions == CaptionsIn::Picture,
            subtitles,
            captions,
            media: self.library.clone(),
        };
        self.export_last = Some((c.format, c.captions));
        match ed.services.export_video(job) {
            Ok(x) => self.export = Some(ExportStep::Making { job: x, path: c.path.clone(), subtitles }),
            Err(e) => {
                self.export = None;
                ed.error(ctx, e);
            }
        }
    }

    /// Whether the video being made is done (every frame).
    pub(super) fn poll_export(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let Some(ExportStep::Making { job, subtitles, .. }) = &self.export else { return };
        let done = job.done.lock().ok().and_then(|mut d| d.take());
        match done {
            Some(Ok(path)) => {
                let subtitles = subtitles.then(|| with_extension(&path, "srt"));
                self.export = Some(ExportStep::Made { path, subtitles });
            }
            Some(Err(e)) => {
                if job.cancel.load(std::sync::atomic::Ordering::SeqCst) {
                    ed.toast(ctx, "Export cancelled: nothing was saved");
                } else {
                    ed.error(ctx, format!("The video could not be made: {e}"));
                }
                self.export = None;
            }
            None => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
        }
    }

    /// The window of the step Export Video is at, over the editor (which waits meanwhile).
    pub(super) fn export_windows(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let Some(step) = self.export.take() else { return };
        let frame = widgets::window_frame(&ctx.global_style());
        let modal = |id: &str| egui::Modal::new(egui::Id::new(id)).frame(frame);
        self.export = match step {
            ExportStep::Choose(mut c) => {
                let mut answer = None;
                modal("export-video").show(ctx, |ui| {
                    ui.set_width(500.0);
                    let closed = widgets::title_bar(ui, if c.format.has_picture() { "Export Video" } else { "Export Sound" });
                    widgets::content(ui, |ui| self.choose(ed, ui, &mut c));
                    let buttons = [FooterButton::primary("Export"), FooterButton::cancel("Cancel")];
                    answer = widgets::dialog_footer(ui, &buttons).and_then(|i| buttons.get(i)).map(|b| b.role);
                    if closed {
                        answer = Some(Role::Cancel);
                    }
                });
                match answer {
                    Some(Role::Default) => {
                        self.start_export(ed, ctx, &c);
                        return;
                    }
                    Some(_) => None,
                    None => Some(ExportStep::Choose(c)),
                }
            }
            ExportStep::Making { job, path, subtitles } => {
                let stopping = job.cancel.load(std::sync::atomic::Ordering::SeqCst);
                let mut stop = false;
                modal("making-video").show(ctx, |ui| {
                    ui.set_width(440.0);
                    stop |= widgets::title_bar(ui, if Made::of(&path) == Made::Sound { "Exporting Sound" } else { "Exporting Video" });
                    widgets::content(ui, |ui| {
                        let t = Tokens::get(ui.ctx());
                        let what = if stopping { "Stopping…".to_string() } else { format!("Making “{}”…", file_name(&path)) };
                        ui.label(RichText::new(what).color(t.text));
                        ui.add_space(10.0);
                        let p = job.progress.load(std::sync::atomic::Ordering::SeqCst).min(1000) as f32 / 1000.0;
                        ui.add(egui::ProgressBar::new(p).show_percentage().animate(!stopping));
                    });
                    let buttons = [FooterButton::cancel("Cancel").enabled(!stopping)];
                    stop |= widgets::dialog_footer(ui, &buttons).is_some();
                });
                if stop {
                    job.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                Some(ExportStep::Making { job, path, subtitles })
            }
            ExportStep::Made { path, subtitles } => {
                let (title, open) = match Made::of(&path) {
                    Made::Gif => ("GIF Exported", "Open GIF"),
                    Made::Sound => ("Sound Exported", "Play Sound"),
                    Made::Video => ("Video Exported", "Play Video"),
                };
                let mut answer = None;
                modal("video-made").show(ctx, |ui| {
                    ui.set_width(460.0);
                    let closed = widgets::title_bar(ui, title);
                    widgets::content(ui, |ui| {
                        let t = Tokens::get(ui.ctx());
                        ui.label(RichText::new(file_name(&path)).strong().size(15.0).color(t.text));
                        ui.label(RichText::new(format!("in {}", folder(&path))).color(t.muted));
                        if let Some(s) = &subtitles {
                            ui.add_space(6.0);
                            ui.label(RichText::new(format!("Its subtitles are beside it: {}", file_name(s))).color(t.muted));
                        }
                    });
                    let buttons = [FooterButton::primary(open), FooterButton::plain("Show in Folder"), FooterButton::cancel("Close")];
                    answer = widgets::dialog_footer(ui, &buttons);
                    if closed {
                        answer = Some(2);
                    }
                });
                match answer {
                    Some(0) => {
                        ed.services.open_with_system(&path);
                        None
                    }
                    Some(1) => {
                        ed.services.open_folder(Some(&path));
                        None
                    }
                    Some(_) => None,
                    None => Some(ExportStep::Made { path, subtitles }),
                }
            }
        };
    }

    /// The Export Video window's settings.
    fn choose(&mut self, ed: &mut Editor, ui: &mut egui::Ui, c: &mut ExportChoice) {
        let t = Tokens::get(ui.ctx());
        let heading = |ui: &mut egui::Ui, s: &str| {
            ui.label(RichText::new(s).strong().color(t.text));
        };
        heading(ui, "Save as");
        ui.horizontal(|ui| {
            ui.label(RichText::new(file_name(&c.path)).color(t.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::button(ui, "Change…").on_hover_text("Choose another folder or name").clicked() {
                    ed.saving = None;
                    ed.saving_project = false;
                    ed.saving_transcript = None;
                    ed.saving_video = true;
                    ed.services.request_save(&file_name(&c.path), &[c.format.filter()]);
                }
            });
        });
        ui.label(RichText::new(format!("in {}", folder(&c.path))).small().color(t.muted));
        ui.add_space(12.0);

        heading(ui, "Format");
        let before = c.format;
        // Every kind, those this computer can't make greyed out with why.
        egui::ComboBox::from_id_salt("export-format").width(ui.available_width()).selected_text(c.format.label()).show_ui(ui, |ui| {
            for f in VideoFormat::ALL {
                let can = c.formats.contains(&f);
                ui.add_enabled_ui(can, |ui| ui.selectable_value(&mut c.format, f, f.label())).inner.on_disabled_hover_text(f.why_not());
            }
        });
        if c.format != before {
            // The name follows the kind of file (a name picked for an MP4 too).
            if std::path::Path::new(&c.path).extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(before.extension())) {
                c.path = with_extension(&c.path, c.format.extension());
            }
            fit_captions(c);
        }
        if c.format == VideoFormat::Mp4Av1 {
            ui.label(RichText::new("Windows Media Player and older devices can't play these.").small().color(t.muted));
        }
        if !c.format.has_picture() {
            ui.label(RichText::new("The sound of the video as edited: every track and sound you added, mixed.").small().color(t.muted));
        }
        ui.add_space(12.0);

        // The chosen stretch alone, or the whole video.
        if let Some((a, b)) = c.stretch {
            heading(ui, "What");
            let whole = format!("The whole video ({})", super::clock(self.session.timeline.length()));
            ui.radio_value(&mut c.only_stretch, false, whole);
            let part = format!("Just the chosen stretch: {} to {} ({})", super::clock(a), super::clock(b), super::clock(b.saturating_sub(a)));
            ui.radio_value(&mut c.only_stretch, true, part);
            ui.add_space(12.0);
        }

        heading(ui, "Closed captions (AI generated)");
        if self.cues.is_empty() {
            let why = if self.captions.is_some() {
                "Nothing said is left in the video as edited."
            } else {
                "This recording has no captions yet: Transcribe (under the tracks) writes down what was said, on this computer."
            };
            ui.label(RichText::new(why).color(t.muted));
            c.captions = CaptionsIn::Off;
        } else {
            for option in [CaptionsIn::Off, CaptionsIn::Picture, CaptionsIn::Subtitles] {
                let (possible, why) = match option {
                    CaptionsIn::Subtitles => (c.format.has_sound(), "A GIF has no sound, so it has no subtitles"),
                    CaptionsIn::Picture => (c.format.has_picture(), "Just the sound has no picture to put them on"),
                    CaptionsIn::Off => (true, ""),
                };
                ui.add_enabled_ui(possible, |ui| ui.radio_value(&mut c.captions, option, option.label())).inner.on_disabled_hover_text(why);
            }
            ui.label(
                RichText::new("Written down on this computer by speech recognition, which can mishear: check them before sharing.")
                    .small()
                    .color(t.muted),
            );
        }
        if c.format.has_picture() {
            ui.add_space(12.0);
            heading(ui, "Watermark");
            self.watermark_choice(ed, ui);
        }
    }

    /// The watermark: on or off, words or a picture of the media bin, its corner, how much it
    /// shows and how large. Each change is an edit (the preview shows it too).
    fn watermark_choice(&mut self, ed: &mut Editor, ui: &mut egui::Ui) {
        use screengrabcraft_media::layers::MediaKind;
        use screengrabcraft_media::timeline::Corner;
        use serde_json::json;
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let w = self.session.timeline.watermark.clone();
        let mut set = None;
        let mut typing = false;
        let mut on = w.on;
        if ui.checkbox(&mut on, "Words or a picture on every frame").changed() {
            set = Some(json!({"on": on}));
        }
        if !w.on {
            if let Some(p) = set {
                self.run(ed, &ctx, "video.watermark", p);
            }
            return;
        }
        let pictures: Vec<(u32, String)> =
            self.session.timeline.media.iter().filter(|m| m.kind == MediaKind::Picture).map(|m| (m.id, m.name.clone())).collect();
        ui.horizontal(|ui| {
            if ui.radio(w.picture.is_none(), "Words").clicked() && w.picture.is_some() {
                set = Some(json!({"picture": null}));
            }
            let first = pictures.first().map(|(id, _)| *id);
            let r = ui.add_enabled(first.is_some(), egui::RadioButton::new(w.picture.is_some(), "A picture from the Media Bin"));
            if r.on_disabled_hover_text("Bring a picture into the Media Bin first (File, Import Media)").clicked()
                && w.picture.is_none()
                && let Some(id) = first
            {
                set = Some(json!({"picture": id}));
            }
        });
        match w.picture {
            None => {
                let mut words = w.words.clone();
                let named = ui.label(RichText::new("The words").small().color(t.muted));
                let r = ui
                    .add(egui::TextEdit::singleline(&mut words).hint_text("Draft, your name, ©…").desired_width(ui.available_width()))
                    .labelled_by(named.id);
                if r.changed() {
                    // Typing is one change.
                    self.session.begin("Watermark");
                    let _ = self.session.drag("video.watermark", &json!({"words": words}));
                }
                if r.lost_focus() {
                    self.session.commit();
                }
                typing = r.has_focus();
            }
            Some(id) => {
                let name = pictures.iter().find(|(p, _)| *p == id).map(|(_, n)| n.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt("watermark-picture").width(ui.available_width()).selected_text(name).show_ui(ui, |ui| {
                    for (p, n) in &pictures {
                        if ui.selectable_label(*p == id, n).clicked() {
                            set = Some(json!({"picture": p}));
                        }
                    }
                });
            }
        }
        ui.horizontal(|ui| {
            for (corner, label, key) in [
                (Corner::TopLeft, "Top left", "topLeft"),
                (Corner::TopRight, "Top right", "topRight"),
                (Corner::BottomLeft, "Bottom left", "bottomLeft"),
                (Corner::BottomRight, "Bottom right", "bottomRight"),
            ] {
                if ui.add(egui::Button::new(label).selected(w.corner == corner)).clicked() {
                    set = Some(json!({"corner": key}));
                }
            }
        });
        for (name, key, value, range) in [
            ("How much it shows", "opacity", w.opacity * 100.0, 5.0..=100.0),
            ("How tall (of the video's height)", "size", w.size * 100.0, 2.0..=30.0),
        ] {
            let mut v = value;
            let r = super::named_slider(ui, name, &mut v, range, "%");
            if r.drag_started() {
                self.session.begin("Watermark");
            }
            // (The slider rounds what it's given: only a move is a change.)
            if r.changed() && (v - value).abs() > 0.01 {
                let p = json!({key: v / 100.0});
                if r.dragged() {
                    let _ = self.session.drag("video.watermark", &p);
                } else {
                    set = Some(p);
                }
            }
            if r.drag_stopped() {
                self.session.commit();
            }
        }
        ui.label(RichText::new("It's on the preview too, and stays with the edits.").small().color(t.muted));
        // Nothing typed or dragged any more: what was is one change, kept.
        if !typing && ui.ctx().dragged_id().is_none() {
            self.session.commit();
        }
        if let Some(p) = set {
            self.run(ed, &ctx, "video.watermark", p);
        }
    }
}
