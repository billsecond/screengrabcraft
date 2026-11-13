//! Transcribe a Video or Sound File…: files picked in the editor (made by another program, say)
//! are transcribed in the background, on this computer, by the capture service. When one is
//! done its transcript (`.txt`) and subtitles (`.srt`) are beside it, and it's shown in its
//! folder. Meanwhile the status bar says how far it has got.
//!
//! A recording of ScreenGrabCraft's is heard again (the service writes the files beside it). A
//! sound file's or a video's captions are kept by the service once made, so asking again for an
//! unchanged file is answered at once; the editor writes the files beside it.

use egui::{RichText, Ui};

use crate::editor::{Editor, file_name};
use crate::theme::Tokens;

/// How often the files being transcribed are looked at (seconds).
const POLL: f64 = 1.0;
/// Given up on when the service hasn't said anything of it for this long (seconds).
const SILENT: f64 = 30.0 * 60.0;

/// A file being transcribed.
#[derive(Clone, Debug)]
pub struct FileJob {
    pub path: String,
    pub name: String,
    /// A recording of ScreenGrabCraft's (its captions are written into it and beside it).
    recording: bool,
    /// The captions' time when it was asked (a recording's earlier ones aren't the answer).
    seen: u64,
    pub progress: Option<f32>,
    /// When the service last said something of it.
    heard: f64,
}

/// The files being transcribed.
#[derive(Debug, Default)]
pub struct Transcripts {
    pub jobs: Vec<FileJob>,
    next_poll: f64,
}

impl Transcripts {
    /// What the status bar says: which file (or how many), and how far it has got.
    pub fn status(&self) -> Option<(String, Option<f32>)> {
        let first = self.jobs.first()?;
        let text = match self.jobs.len() {
            1 => format!("Transcribing {}…", first.name),
            n => format!("Transcribing {n} files…"),
        };
        Some((text, first.progress))
    }
}

/// The file beside `path` with another extension.
fn beside(path: &str, ext: &str) -> String {
    std::path::Path::new(path).with_extension(ext).to_string_lossy().to_string()
}

/// A spinner, the words and how far it has got: the status bar's word on transcribing.
pub fn indicator(ui: &mut Ui, text: &str, progress: Option<f32>) {
    let t = Tokens::get(ui.ctx());
    // Laid out right to left: the bar, then the words, then the spinner.
    if let Some(p) = progress {
        ui.add(egui::ProgressBar::new(p).desired_width(90.0).desired_height(8.0).fill(t.accent));
    }
    let said = match progress {
        Some(p) => format!("{text} {:.0}%", p * 100.0),
        None => text.to_string(),
    };
    ui.label(RichText::new(said).size(12.0).color(t.text));
    ui.add(egui::Spinner::new().size(12.0));
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
}

impl Editor {
    /// Transcribe a Video or Sound File…: the files picked are asked for.
    pub fn transcribe_files(&mut self, ctx: &egui::Context, paths: &[String]) {
        let now = ctx.input(|i| i.time);
        for path in paths.iter().take(20) {
            let name = file_name(path);
            if self.transcripts.jobs.iter().any(|j| &j.path == path) {
                continue;
            }
            let lower = path.to_ascii_lowercase();
            let recording = lower.ends_with(&format!(".{}", screengrabcraft_media::recording::EXT));
            let hearable = recording
                || matches!(
                    screengrabcraft_media::media_store::kind_of(path),
                    Some(screengrabcraft_media::layers::MediaKind::Sound | screengrabcraft_media::layers::MediaKind::Video)
                );
            if !hearable {
                self.error(ctx, format!("{name}: not a video or a sound file"));
                continue;
            }
            // A recording's captions as they are now aren't the answer: it's heard again.
            let (asked, seen) = if recording {
                let seen = self.services.poll_captions(path, 0).map_or(0, |(s, _)| s);
                (self.services.make_captions_again(path), seen)
            } else {
                (self.services.make_captions(path), 0)
            };
            match asked {
                Ok(()) => self.transcripts.jobs.push(FileJob { path: path.clone(), name, recording, seen, progress: None, heard: now }),
                Err(e) => self.error(ctx, format!("{name}: {e}")),
            }
        }
        self.transcripts.next_poll = now;
    }

    /// The files being transcribed: how far they've got, and those done written beside them.
    pub(crate) fn poll_transcripts(&mut self, ctx: &egui::Context) {
        if self.transcripts.jobs.is_empty() {
            return;
        }
        let now = ctx.input(|i| i.time);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(POLL));
        if now < self.transcripts.next_poll {
            return;
        }
        self.transcripts.next_poll = now + POLL;
        let jobs = std::mem::take(&mut self.transcripts.jobs);
        for mut job in jobs {
            match self.services.captions_progress(&job.path) {
                Some(Ok(p)) => {
                    job.progress = Some(p);
                    job.heard = now;
                }
                Some(Err(why)) => {
                    self.error(ctx, format!("{} couldn't be transcribed: {why}", job.name));
                    continue;
                }
                None => {}
            }
            if job.recording {
                if let Some((_, caps)) = self.services.poll_captions(&job.path, job.seen) {
                    match caps.filter(|c| !c.is_empty()) {
                        Some(_) => {
                            let txt = beside(&job.path, "txt");
                            self.toast(ctx, format!("Transcribed {}: {} is beside it", job.name, file_name(&txt)));
                            self.services.open_folder(Some(&txt));
                        }
                        None => self.toast(ctx, format!("No words were heard in {}", job.name)),
                    }
                    continue;
                }
            } else if let Some((_, cues)) = self.services.poll_media_captions(&job.path, job.seen) {
                self.media_transcribed(ctx, &job, &cues);
                continue;
            }
            if now - job.heard > SILENT {
                self.error(ctx, format!("{} wasn't transcribed: the capture service didn't take it on", job.name));
                continue;
            }
            self.transcripts.jobs.push(job);
        }
    }

    /// A sound file's or a video's lines have come: its transcript and subtitles beside it.
    fn media_transcribed(&mut self, ctx: &egui::Context, job: &FileJob, cues: &[screengrabcraft_media::captions::Cue]) {
        use screengrabcraft_media::captions;
        if cues.is_empty() {
            self.toast(ctx, format!("No words were heard in {}", job.name));
            return;
        }
        let (srt, txt) = (beside(&job.path, "srt"), beside(&job.path, "txt"));
        let written = self
            .services
            .write_file(&srt, captions::to_srt(cues).as_bytes())
            .and_then(|()| self.services.write_file(&txt, captions::to_text(cues).as_bytes()));
        match written {
            Ok(()) => {
                self.toast(ctx, format!("Transcribed {}: {} and {} are beside it", job.name, file_name(&txt), file_name(&srt)));
                self.services.open_folder(Some(&txt));
            }
            Err(e) => self.error(ctx, format!("{}'s transcript couldn't be written beside it: {e}", job.name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use screengrabcraft_media::captions::{Cue, Speaker};
    use screengrabcraft_model::EditorSettings;

    use super::*;
    use crate::services::Services;

    /// A capture service that transcribes when told: progress, then the lines.
    #[derive(Clone, Default)]
    struct Host {
        asked: Arc<Mutex<Vec<String>>>,
        progress: Arc<Mutex<Option<Result<f32, String>>>>,
        lines: Arc<Mutex<Option<Vec<Cue>>>>,
        written: Arc<Mutex<Vec<(String, String)>>>,
        shown: Arc<Mutex<Vec<String>>>,
    }

    impl Services for Host {
        fn make_captions(&self, path: &str) -> Result<(), String> {
            self.asked.lock().unwrap().push(path.to_string());
            Ok(())
        }
        fn captions_progress(&self, _path: &str) -> Option<Result<f32, String>> {
            self.progress.lock().unwrap().clone()
        }
        fn poll_media_captions(&self, _path: &str, seen: u64) -> Option<(u64, Vec<Cue>)> {
            let lines = self.lines.lock().unwrap().clone()?;
            (seen != 9).then_some((9, lines))
        }
        fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
            self.written.lock().unwrap().push((path.to_string(), String::from_utf8_lossy(bytes).to_string()));
            Ok(())
        }
        fn open_folder(&self, path: Option<&str>) {
            self.shown.lock().unwrap().push(path.unwrap_or("").to_string());
        }
    }

    #[test]
    fn a_video_from_another_program_is_transcribed_beside_it() {
        let host = Host::default();
        let mut ed = Editor::new(Box::new(host.clone()), EditorSettings::default());
        let ctx = egui::Context::default();
        ed.transcribe_files(&ctx, &["C:/Videos/Interview.mp4".to_string(), "C:/Notes/notes.txt".to_string()]);
        assert_eq!(host.asked.lock().unwrap().as_slice(), ["C:/Videos/Interview.mp4"], "a text file isn't transcribed");
        // Meanwhile: how far it has got.
        *host.progress.lock().unwrap() = Some(Ok(0.42));
        ed.poll_transcripts(&ctx);
        assert_eq!(ed.transcripts.status(), Some(("Transcribing Interview.mp4…".to_string(), Some(0.42))));
        // Done: its transcript and subtitles beside it, shown in its folder.
        *host.progress.lock().unwrap() = None;
        *host.lines.lock().unwrap() = Some(vec![Cue { start: 1_000_000, end: 2_000_000, speaker: Speaker::Media, text: "Welcome back.".into() }]);
        ed.transcripts.next_poll = 0.0;
        ed.poll_transcripts(&ctx);
        assert!(ed.transcripts.jobs.is_empty());
        let written = host.written.lock().unwrap().clone();
        assert_eq!(written[0], ("C:/Videos/Interview.srt".to_string(), "1\n00:00:01,000 --> 00:00:02,000\nWelcome back.\n\n".to_string()));
        assert_eq!(written[1], ("C:/Videos/Interview.txt".to_string(), "[0:00:01] Welcome back.\n".to_string()));
        assert_eq!(host.shown.lock().unwrap().as_slice(), ["C:/Videos/Interview.txt"]);
    }

    #[test]
    fn a_file_that_cant_be_transcribed_says_why_and_is_let_go() {
        let host = Host::default();
        let mut ed = Editor::new(Box::new(host.clone()), EditorSettings::default());
        let ctx = egui::Context::default();
        ed.transcribe_files(&ctx, &["C:/Videos/Silent.mp4".to_string()]);
        *host.progress.lock().unwrap() = Some(Err("it has no sound".into()));
        ed.poll_transcripts(&ctx);
        assert!(ed.transcripts.jobs.is_empty() && ed.transcripts.status().is_none());
        assert!(host.written.lock().unwrap().is_empty());
    }
}
