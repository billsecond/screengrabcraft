//! Export Video: a window to choose where the video goes, what it's made as and whether its
//! captions go with it; a window while it's being made, with Cancel; and one when it's made, to
//! play it or show it selected in its folder.

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
            Some((f, c)) if formats.contains(&f) => (f, c),
            _ => (first, usual),
        };
        let captions = match (have_captions, captions) {
            (false, _) => CaptionsIn::Off,
            (true, CaptionsIn::Subtitles) if !format.has_sound() => CaptionsIn::Picture,
            (true, c) => c,
        };
        let path = with_extension(&recording, format.extension());
        self.export = Some(ExportStep::Choose(ExportChoice { path, chosen: false, format, captions, formats }));
    }

    /// The save dialog Export Video's Change… opened has answered: the video goes there.
    pub fn export_saved_to(&mut self, path: &str) {
        let Some(ExportStep::Choose(c)) = self.export.as_mut() else { return };
        let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        match ext.as_deref() {
            Some("gif") if c.formats.contains(&VideoFormat::Gif) => c.format = VideoFormat::Gif,
            Some("mp4") if c.format == VideoFormat::Gif => {
                if let Some(mp4) = c.formats.iter().copied().find(|f| *f != VideoFormat::Gif) {
                    c.format = mp4;
                }
            }
            _ => {}
        }
        if !c.format.has_sound() && c.captions == CaptionsIn::Subtitles {
            c.captions = CaptionsIn::Picture;
        }
        // A name without the extension of what's made gets it.
        c.path = if ext.as_deref() == Some(c.format.extension()) { path.to_string() } else { format!("{path}.{}", c.format.extension()) };
        c.chosen = true;
    }

    /// Starts making the video as chosen.
    fn start_export(&mut self, ed: &mut Editor, ctx: &egui::Context, c: &ExportChoice) {
        let captions = (!self.cues.is_empty() && c.captions != CaptionsIn::Off).then(|| self.cues.to_vec());
        let subtitles = captions.is_some() && c.captions == CaptionsIn::Subtitles && c.format.has_sound();
        let job = VideoJob {
            recording: self.rec.clone(),
            timeline: self.session.timeline.clone(),
            out: c.path.clone(),
            replace: c.chosen,
            format: c.format,
            max_size: (1920, 1080),
            fps: self.rec.header.fps.clamp(5, 60),
            burn: captions.is_some() && c.captions == CaptionsIn::Picture,
            subtitles,
            captions,
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
                    let closed = widgets::title_bar(ui, "Export Video");
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
                    stop |= widgets::title_bar(ui, "Exporting Video");
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
                let gif = path.to_ascii_lowercase().ends_with(".gif");
                let mut answer = None;
                modal("video-made").show(ctx, |ui| {
                    ui.set_width(460.0);
                    let closed = widgets::title_bar(ui, if gif { "GIF Exported" } else { "Video Exported" });
                    widgets::content(ui, |ui| {
                        let t = Tokens::get(ui.ctx());
                        ui.label(RichText::new(file_name(&path)).strong().size(15.0).color(t.text));
                        ui.label(RichText::new(format!("in {}", folder(&path))).color(t.muted));
                        if let Some(s) = &subtitles {
                            ui.add_space(6.0);
                            ui.label(RichText::new(format!("Its subtitles are beside it: {}", file_name(s))).color(t.muted));
                        }
                    });
                    let buttons = [
                        FooterButton::primary(if gif { "Open GIF" } else { "Play Video" }),
                        FooterButton::plain("Show in Folder"),
                        FooterButton::cancel("Close"),
                    ];
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
        egui::ComboBox::from_id_salt("export-format").width(ui.available_width()).selected_text(c.format.label()).show_ui(ui, |ui| {
            for f in c.formats.clone() {
                ui.selectable_value(&mut c.format, f, f.label());
            }
        });
        if c.format != before {
            // The name follows the kind of file (a name picked for an MP4 too).
            if std::path::Path::new(&c.path).extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(before.extension())) {
                c.path = with_extension(&c.path, c.format.extension());
            }
            if !c.format.has_sound() && c.captions == CaptionsIn::Subtitles {
                c.captions = CaptionsIn::Picture;
            }
        }
        if c.format == VideoFormat::Mp4Av1 {
            ui.label(RichText::new("Windows Media Player and older devices can't play these.").small().color(t.muted));
        }
        ui.add_space(12.0);

        heading(ui, "Closed captions (AI generated)");
        if self.cues.is_empty() {
            let why = if self.captions.is_some() {
                "Nothing said is left in the video as edited."
            } else {
                "This recording has no captions yet: Make Captions (under the tracks) writes down what was said, on this computer."
            };
            ui.label(RichText::new(why).color(t.muted));
            c.captions = CaptionsIn::Off;
        } else {
            for option in [CaptionsIn::Off, CaptionsIn::Picture, CaptionsIn::Subtitles] {
                let possible = option != CaptionsIn::Subtitles || c.format.has_sound();
                ui.add_enabled_ui(possible, |ui| ui.radio_value(&mut c.captions, option, option.label()))
                    .inner
                    .on_disabled_hover_text("A GIF has no sound, so it has no subtitles");
            }
            ui.label(
                RichText::new("Written down on this computer by speech recognition, which can mishear: check them before sharing.")
                    .small()
                    .color(t.muted),
            );
        }
    }
}
