//! The last file opened again. An editor started with nothing to open (from the Start menu, the
//! tray's Open Editor, a desktop icon) shows what it last had open: the Image Editor its last
//! picture or project, the Video Editor its last recording.
//!
//! The window comes up first: the file is opened on the frame after the first one is drawn, so
//! reading it never holds the window up. Anything opened meanwhile (a file on the command line, a
//! capture handed over) wins, and the last file is let be. One that's gone, can't be read or
//! isn't of the editor's kind is passed over without a word, for the one before it: the one the
//! editor last had open, then its recent files, then the newest captures of its kind.

use crate::editor::{Editor, EditorKind, file_name, is_recording_path};
use crate::services::OpenedFile;

/// Where opening the last file again has got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reopen {
    /// Nothing to open again.
    #[default]
    Off,
    /// Asked for: opened once the window has been drawn.
    Asked,
    /// The window has been drawn on this frame (egui's count): opened on a later one. Not on a
    /// second pass of the same frame, which egui may draw before the window is shown.
    Drawn(u64),
}

/// The most files tried for one to open again (each is looked for, the pictures read).
const MAX_TRIED: usize = 12;

impl Editor {
    /// Opens again the last file of this editor's kind once the window is up, if nothing else
    /// is open by then. The host asks when the window is opened with nothing to open.
    pub fn reopen_last(&mut self) {
        if self.nothing_open() {
            self.reopen = Reopen::Asked;
        }
    }

    /// Nothing open: the empty picture the window started with, nothing drawn on it, nothing
    /// being opened or waiting to be.
    pub(crate) fn nothing_open(&self) -> bool {
        self.video.is_none()
            && self.opening_video.is_none()
            && self.picture_path.is_none()
            && self.project_path.is_none()
            && self.pending.is_none()
            && self.dialog.is_none()
            && self.editing.is_none()
            && !self.session.can_undo()
            && self.session.doc.objects.is_empty()
    }

    /// Notes `path` as opened or saved: first in the recent files, and the last file of its kind
    /// (a recording the Video Editor's, a picture or project the Image Editor's; a made video
    /// neither, it plays in the system's player).
    pub(crate) fn remember_file(&mut self, path: &str) {
        self.settings.remember(path);
        if is_recording_path(path) {
            self.settings.last_video = Some(path.to_string());
        } else if !crate::strip::is_video(path) {
            self.settings.last_picture = Some(path.to_string());
        }
    }

    /// Whether this editor opens `path` again: its own kind of file.
    fn reopens(&self, path: &str) -> bool {
        match self.kind {
            EditorKind::Video => is_recording_path(path),
            EditorKind::Pictures => !crate::strip::is_video(path),
        }
    }

    /// The files to try, best first, each once: the one this editor last had open, then its
    /// recent files, then the newest captures of its kind.
    pub(crate) fn last_files(&self) -> Vec<String> {
        let last = match self.kind {
            EditorKind::Video => self.settings.last_video.clone(),
            EditorKind::Pictures => self.settings.last_picture.clone(),
        };
        let mut captures = self.services.recent();
        captures.sort_by_key(|c| std::cmp::Reverse(c.modified));
        let mut out: Vec<String> = Vec::new();
        for p in last.into_iter().chain(self.settings.recent.iter().cloned()).chain(captures.into_iter().map(|c| c.path)) {
            if out.len() >= MAX_TRIED {
                break;
            }
            if !p.is_empty() && self.reopens(&p) && !out.contains(&p) {
                out.push(p);
            }
        }
        out
    }

    /// Once a frame: the last file is opened on the frame after the window was first drawn.
    pub(crate) fn reopen_step(&mut self, ctx: &egui::Context) {
        let Reopen::Drawn(frame) = self.reopen else { return };
        if ctx.cumulative_frame_nr() <= frame {
            return;
        }
        self.reopen = Reopen::Off;
        let files = self.last_files();
        self.reopen_first(ctx, files);
    }

    /// The window has been drawn with the last file still to open: it's opened on the next frame.
    pub(crate) fn reopen_drawn(&mut self, ctx: &egui::Context) {
        if self.reopen == Reopen::Asked {
            self.reopen = Reopen::Drawn(ctx.cumulative_frame_nr());
        }
        if self.reopen != Reopen::Off {
            ctx.request_repaint();
        }
    }

    /// Opens the first of `files` that is there and can be read, saying nothing of those that
    /// aren't. A recording is read on a thread; if it can't be, the rest are tried then.
    pub(crate) fn reopen_first(&mut self, ctx: &egui::Context, files: Vec<String>) {
        if !self.nothing_open() {
            return;
        }
        let mut files = files.into_iter().take(MAX_TRIED);
        while let Some(path) = files.next() {
            if !self.services.file_exists(&path) {
                continue;
            }
            let opened =
                if is_recording_path(&path) { self.reopen_recording(ctx, &path, files.clone().collect()) } else { self.reopen_picture(ctx, &path) };
            if opened {
                log::info!("start: opened again what was last open ({})", file_name(&path));
                return;
            }
        }
    }

    /// A picture or project opened again: true if it opened. One that can't be read leaves no
    /// notice behind.
    fn reopen_picture(&mut self, ctx: &egui::Context, path: &str) -> bool {
        let Ok(bytes) = self.services.read_file(path) else { return false };
        // A recording by another name is the Video Editor's.
        if screengrabcraft_media::recording::sniff(&bytes) {
            return false;
        }
        let before = self.toast.clone();
        self.open_now(ctx, OpenedFile { name: file_name(path), path: Some(path.to_string()), bytes });
        let opened = self.picture_path.as_deref() == Some(path) || self.project_path.as_deref() == Some(path);
        if !opened {
            self.toast = before;
        }
        opened
    }

    /// A recording opened again, read on a thread: true if it's being read (`next` are tried if
    /// it turns out it can't be).
    #[cfg(not(target_arch = "wasm32"))]
    fn reopen_recording(&mut self, ctx: &egui::Context, path: &str, next: Vec<String>) -> bool {
        let Ok(source) = self.services.open_recording(path) else { return false };
        let before = self.toast.clone();
        self.open_video_in_background(ctx, source, path);
        match self.opening_video.as_mut() {
            Some(o) => {
                o.next = Some(next);
                true
            }
            None => {
                self.toast = before;
                false
            }
        }
    }

    /// In a browser, a recording is read at once.
    #[cfg(target_arch = "wasm32")]
    fn reopen_recording(&mut self, ctx: &egui::Context, path: &str, _next: Vec<String>) -> bool {
        let Ok(source) = self.services.open_recording(path) else { return false };
        let before = self.toast.clone();
        self.open_video(ctx, source, Some(path.to_string()), file_name(path));
        let opened = self.video.is_some();
        if !opened {
            self.toast = before;
        }
        opened
    }
}
