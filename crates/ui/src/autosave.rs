//! Saving as the owner works, with nothing to remember and nothing to answer: about a second
//! after the last change (never in the middle of a drag, nor while text is being typed), the
//! side file and the picture itself are written again.
//!
//! - The side file (`capture.sgcraft` beside `capture.png`) holds the document, still editable,
//!   and the picture as first captured, so the owner can always go back to it.
//! - The picture is rewritten with the annotations drawn in, in its own format. It is drawn from
//!   the document each time, never read back, so a JPEG doesn't wear out save after save. A GIF
//!   is never rewritten: a recording would lose its frames.
//!
//! The host writes the files ([`Services::start_save`]), on a thread of its own where it has
//! one, so the window never waits for an encoder. A document with no file gets one in the
//! captures folder on its first change. Before the window closes or another picture takes its
//! place, the save under way finishes; if it fails, the owner is asked what to do.
//!
//! [`Services::start_save`]: crate::services::Services::start_save

use std::time::Duration;

use screengrabcraft_model::{Document, ImageFormat, Raster};
use screengrabcraft_render::project;

use crate::editor::{Editor, Pending, file_name, sibling_project};
use crate::services::OpenedFile;

/// How long the document stays unchanged before it is written.
pub const QUIET_SECS: f64 = 1.0;
/// The quality a JPEG is written again at.
const JPEG_QUALITY: u8 = 92;

/// Where the files go when deleted: what Windows and the others call it.
pub const TRASH: &str = if cfg!(target_os = "windows") { "Recycle Bin" } else { "Trash" };

/// One save: the files it writes, and what goes in them.
#[derive(Clone, Debug)]
pub struct SaveJob {
    pub id: u64,
    pub doc: Document,
    /// The picture as first captured, if known.
    pub original: Option<Raster>,
    /// The side file, or the project the document was opened from.
    pub project: String,
    /// The picture, written again with the annotations drawn in; none for a project on its own
    /// or a picture kept as it is (a GIF).
    pub picture: Option<(String, ImageFormat)>,
}

/// A save that finished.
#[derive(Clone, Debug)]
pub struct Saved {
    pub id: u64,
    pub result: Result<(), String>,
}

impl SaveJob {
    /// Draws, encodes and writes, through `write`. The side file goes first: the picture is
    /// only written again once the side file holds the same.
    pub fn run(&self, write: &dyn Fn(&str, &[u8]) -> Result<(), String>) -> Saved {
        Saved { id: self.id, result: self.write(write) }
    }

    fn write(&self, write: &dyn Fn(&str, &[u8]) -> Result<(), String>) -> Result<(), String> {
        let bytes = project::save_with_original(&self.doc, self.original.as_ref()).map_err(|e| e.to_string())?;
        write(&self.project, &bytes).map_err(|e| format!("{}: {e}", file_name(&self.project)))?;
        let Some((path, format)) = &self.picture else { return Ok(()) };
        let flat = screengrabcraft_render::render(&self.doc).ok_or_else(|| "the picture could not be drawn".to_string())?;
        let bytes = screengrabcraft_render::encode(&flat, *format, JPEG_QUALITY).map_err(|e| e.to_string())?;
        write(path, &bytes).map_err(|e| format!("{}: {e}", file_name(path)))
    }
}

/// Where saving stands, for the status bar.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    #[default]
    Idle,
    Saving,
    Saved,
    Failed(String),
}

/// When to save: the quiet second after a change, the save under way, the last that failed.
#[derive(Debug, Default)]
pub struct Saver {
    /// The document's revision when last seen, and when it moved to it.
    seen: u64,
    seen_at: f64,
    /// The save under way: its id and the revision it writes.
    writing: Option<(u64, u64)>,
    next_id: u64,
    /// The revision whose save failed, and why.
    failed: Option<(u64, String)>,
    /// Save as soon as nothing is being written, without waiting for quiet: something waits
    /// on it (closing, opening another picture).
    pub(crate) urgent: bool,
    /// Write even if nothing changed (Ctrl+S).
    force: bool,
    /// The save to say "Saved" out loud for (Ctrl+S).
    announce: Option<u64>,
    pub status: Status,
    /// What the status bar says once, in place of "Saved" (a GIF kept as it is).
    pub(crate) note: Option<String>,
    note_said: bool,
    note_shown: bool,
}

impl Saver {
    /// Notes the document's revision: the quiet second starts again whenever it moves.
    pub fn watch(&mut self, revision: u64, now: f64) {
        if revision != self.seen {
            self.seen = revision;
            self.seen_at = now;
        }
    }

    fn failed_now(&self) -> bool {
        self.failed.as_ref().is_some_and(|(r, _)| *r == self.seen)
    }

    /// Whether a save should start now. `unsaved`: there are changes not yet written; `busy`: a
    /// drag or typing is under way.
    pub fn due(&self, unsaved: bool, now: f64, busy: bool) -> bool {
        if self.writing.is_some() {
            return false;
        }
        if self.force {
            return true;
        }
        if self.urgent {
            return unsaved;
        }
        unsaved && !busy && !self.failed_now() && now - self.seen_at >= QUIET_SECS
    }

    /// How long until a save is due (to wake up for), if one will be.
    pub fn wait(&self, unsaved: bool, now: f64, busy: bool) -> Option<f64> {
        (unsaved && !busy && self.writing.is_none() && !self.failed_now()).then(|| (QUIET_SECS - (now - self.seen_at)).max(0.0))
    }

    /// A save of `revision` starts; its id.
    pub fn start(&mut self, revision: u64) -> u64 {
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        self.writing = Some((id, revision));
        if self.force {
            self.announce = Some(id);
        }
        // Said with one "Saved"; the next is plain.
        if self.note_shown {
            self.note = None;
        }
        self.urgent = false;
        self.force = false;
        self.status = Status::Saving;
        id
    }

    /// A save of `revision` couldn't start (there's nowhere to write it).
    pub fn fail(&mut self, revision: u64, error: String) {
        self.urgent = false;
        self.force = false;
        self.status = Status::Failed(error.clone());
        self.failed = Some((revision, error));
    }

    /// A save finished. The revision it wrote, if it's the one under way, and whether it was
    /// asked for out loud.
    pub fn finish(&mut self, done: &Saved) -> Option<(u64, bool)> {
        let (id, revision) = self.writing.filter(|(id, _)| *id == done.id)?;
        self.writing = None;
        match &done.result {
            Ok(()) => {
                self.failed = None;
                self.status = Status::Saved;
                self.note_shown = self.note.is_some();
            }
            Err(e) => {
                self.failed = Some((revision, e.clone()));
                self.status = Status::Failed(e.clone());
            }
        }
        let announce = self.announce == Some(id);
        if announce {
            self.announce = None;
        }
        Some((revision, announce))
    }

    /// Ctrl+S: write now, changed or not.
    pub fn save_now(&mut self) {
        self.force = true;
    }

    /// Whether a save is being written.
    pub fn writing(&self) -> bool {
        self.writing.is_some()
    }

    /// Nothing waiting to be written and nothing being written.
    pub fn settled(&self, unsaved: bool) -> bool {
        !unsaved && self.writing.is_none() && !self.force
    }

    /// Why the changes couldn't be written, when the last try failed and nothing else is under way.
    pub fn problem(&self, unsaved: bool) -> Option<&str> {
        if !unsaved || self.writing.is_some() {
            return None;
        }
        self.failed.as_ref().filter(|(r, _)| *r == self.seen).map(|(_, e)| e.as_str())
    }

    /// Another document: it starts out saved. A save still under way goes on.
    pub fn reset(&mut self) {
        self.failed = None;
        self.urgent = false;
        self.force = false;
        self.announce = None;
        self.status = Status::Idle;
        self.note = None;
        self.note_said = false;
        self.note_shown = false;
    }

    /// The document was saved some other way (Save As).
    pub fn saved_elsewhere(&mut self) {
        self.failed = None;
        self.force = false;
        self.status = Status::Saved;
    }

    /// The owner chose to let the unsaved changes go.
    pub fn discard(&mut self) {
        self.failed = None;
        self.urgent = false;
        self.force = false;
        self.status = Status::Idle;
    }

    /// Says `text` once in the status bar, with the next "Saved".
    fn say_once(&mut self, text: String) {
        if !self.note_said {
            self.note_said = true;
            self.note = Some(text);
        }
    }
}

impl Editor {
    /// Whether the editor writes its files as the owner works: a host with paths (the
    /// desktop). Without them (the browser) saving is a download, asked for.
    pub fn autosaves(&self) -> bool {
        self.services.has_paths()
    }

    /// A drag, typing, or a change in the properties is under way: no save cuts in.
    fn busy(&self) -> bool {
        self.session.editing() || self.drag.is_some() || self.editing.is_some() || self.props_editing
    }

    /// Ends typing, a drag or a change in the properties, so the document is whole.
    pub(crate) fn finish_edits(&mut self) {
        self.stop_editing();
        self.drag = None;
        self.props_editing = false;
        self.session.commit();
    }

    /// Whether the document may be replaced (or the window closed) now. If not, it's being
    /// saved first; without files to save to, the owner is asked.
    pub(crate) fn ready_to_leave(&mut self) -> bool {
        if !self.autosaves() {
            return !self.session.dirty;
        }
        self.finish_edits();
        if self.saver.settled(self.session.dirty) && self.trashing.is_empty() {
            return true;
        }
        self.saver.urgent = true;
        false
    }

    /// Ctrl+S: the files now (a document with no file gets one), or, without files, a download.
    pub fn save(&mut self, _ctx: &egui::Context) {
        if self.autosaves() {
            self.saver.save_now();
        } else {
            self.save_as();
        }
    }

    /// Each frame: saves that finished, a save that's due, and what was waiting on one.
    pub(crate) fn autosave(&mut self, ctx: &egui::Context) {
        if !self.autosaves() {
            return;
        }
        while let Some(done) = self.services.poll_saved() {
            self.saved(ctx, done);
        }
        let now = ctx.input(|i| i.time);
        self.saver.watch(self.session.revision(), now);
        let (unsaved, busy) = (self.session.dirty, self.busy());
        if self.pending.is_some() && self.saver.problem(unsaved).is_none() {
            self.saver.urgent = true;
        }
        if self.saver.due(unsaved, now, busy) {
            self.start_save(ctx);
        } else if let Some(secs) = self.saver.wait(unsaved, now, busy) {
            ctx.request_repaint_after(Duration::try_from_secs_f64(secs).unwrap_or_default());
        }
        self.go_on_when_saved(ctx);
        self.trash_when_quiet(ctx);
    }

    fn start_save(&mut self, ctx: &egui::Context) {
        let revision = self.session.revision();
        let (project, picture) = match self.save_targets() {
            Ok(t) => t,
            Err(e) => {
                self.saver.fail(revision, e.clone());
                self.error(ctx, format!("Your changes couldn't be saved: {e}"));
                return;
            }
        };
        let id = self.saver.start(revision);
        let job = SaveJob { id, doc: self.session.doc.clone(), original: self.original.clone(), project, picture };
        if let Some(done) = self.services.start_save(job) {
            self.saved(ctx, done);
        }
    }

    /// Where the document is written: its project (the side file beside a picture), and the
    /// picture itself unless it's kept as it is. A document with no file gets one.
    fn save_targets(&mut self) -> Result<(String, Option<(String, ImageFormat)>), String> {
        if self.picture_path.is_none() && self.project_path.is_none() {
            let source = self.session.doc.meta.source.clone().unwrap_or_else(|| self.name.clone());
            let path = self.services.new_picture_path(&source)?;
            self.name = stem(&path);
            self.picture_path = Some(path.clone());
            self.settings.remember(&path);
            self.services.save_settings(&self.settings);
        }
        let Some(picture) = self.picture_path.clone() else {
            return self.project_path.clone().map(|p| (p, None)).ok_or_else(|| "there is no file to save to".to_string());
        };
        let project = self.project_path.clone().unwrap_or_else(|| sibling_project(&picture));
        let format = std::path::Path::new(&picture)
            .extension()
            .and_then(|e| e.to_str())
            .and_then(ImageFormat::from_extension)
            .filter(|f| matches!(f, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Webp | ImageFormat::Bmp));
        if format.is_none() {
            self.saver.say_once(format!("{} is kept as it is; the changes are in {}", file_name(&picture), file_name(&project)));
        }
        Ok((project, format.map(|f| (picture, f))))
    }

    /// A save finished.
    fn saved(&mut self, ctx: &egui::Context, done: Saved) {
        let Some((revision, announce)) = self.saver.finish(&done) else { return };
        match done.result {
            Ok(()) => {
                if self.session.revision() == revision {
                    self.session.dirty = false;
                }
                // The strip shows the picture as it is now.
                self.recent_at = f64::NEG_INFINITY;
                if announce {
                    let name = self.picture_path.as_deref().or(self.project_path.as_deref()).map(file_name).unwrap_or_default();
                    self.toast(ctx, format!("Saved {name}"));
                }
            }
            Err(e) => self.error(ctx, format!("Your changes couldn't be saved: {e}")),
        }
    }

    /// Goes on with what waited for the save, once it's done; asks what to do if it failed.
    fn go_on_when_saved(&mut self, ctx: &egui::Context) {
        if self.pending.is_none() || self.saving_project {
            return;
        }
        if self.saver.settled(self.session.dirty) && self.trashing.is_empty() {
            self.save_problem = None;
            // Nothing waits any more: the next change waits for its quiet second again.
            self.saver.urgent = false;
            if let Some(p) = self.pending.take() {
                self.go_on(ctx, p);
            }
        } else if let Some(e) = self.saver.problem(self.session.dirty) {
            self.save_problem = Some(e.to_string());
        }
    }

    /// Whether `path` (a picture in the strip) is the document open, or its side file is.
    pub(crate) fn is_open(&self, path: &str) -> bool {
        let side = sibling_project(path);
        self.picture_path.as_deref() == Some(path) || self.project_path.as_deref().is_some_and(|p| p == path || p == side)
    }

    /// The hover button in the strip: moves a capture (and its side file) to the Recycle Bin,
    /// once nothing is being written. Recoverable, so nothing is asked.
    pub fn delete_capture(&mut self, path: &str) {
        if self.trashing.iter().any(|p| p == path) {
            return;
        }
        if self.is_open(path) {
            self.finish_edits();
        }
        self.trashing.push(path.to_string());
    }

    fn trash_when_quiet(&mut self, ctx: &egui::Context) {
        let Some(path) = self.trashing.first().cloned() else { return };
        // A save under way could write the file back after it went.
        if self.saver.writing() {
            return;
        }
        let open = self.is_open(&path);
        // The open capture goes with its last changes in it.
        if open && !self.saver.settled(self.session.dirty) && self.saver.problem(self.session.dirty).is_none() {
            self.saver.urgent = true;
            return;
        }
        self.trashing.remove(0);
        if self.pending.is_none() {
            self.saver.urgent = false;
        }
        self.trash_now(ctx, &path, open);
    }

    fn trash_now(&mut self, ctx: &egui::Context, path: &str, open: bool) {
        let name = file_name(path);
        // The picture first: if it stays, so does everything.
        if let Err(e) = self.services.trash(path) {
            self.error(ctx, format!("{name} couldn't be moved to the {TRASH}: {e}"));
            return;
        }
        let side = sibling_project(path);
        match self.services.trash(&side) {
            Ok(()) => self.toast(ctx, format!("Moved {name} to the {TRASH}")),
            Err(e) => self.error(ctx, format!("Moved {name} to the {TRASH}, but not {}: {e}", file_name(&side))),
        }
        let next = self.next_in_strip(path);
        self.recent.retain(|r| r.path != path);
        self.thumbs.remove(path);
        self.recent_at = f64::NEG_INFINITY;
        if open {
            // It has no file any more: nothing of it is written again.
            self.picture_path = None;
            self.project_path = None;
            self.session.dirty = false;
            self.saver.discard();
            self.open_next(ctx, next);
        }
    }

    /// The capture after `path` in the strip, or before it when it was the last.
    fn next_in_strip(&self, path: &str) -> Option<String> {
        let i = self.recent.iter().position(|r| r.path == path)?;
        let after = i.checked_add(1).and_then(|n| self.recent.get(n));
        let before = i.checked_sub(1).and_then(|n| self.recent.get(n));
        after.or(before).map(|r| r.path.clone())
    }

    /// Opens the next capture after a delete, or an empty document if there's none.
    fn open_next(&mut self, ctx: &egui::Context, next: Option<String>) {
        if let Some(p) = next {
            match self.services.read_file(&p) {
                Ok(bytes) => self.open_now(ctx, OpenedFile { name: file_name(&p), path: Some(p), bytes }),
                Err(e) => self.error(ctx, format!("{}: {e}", file_name(&p))),
            }
        }
        if self.picture_path.is_none() && self.project_path.is_none() {
            self.blank_document();
        }
    }

    /// File → Revert to Original: possible while the original is known and the document
    /// differs from it.
    pub fn can_revert(&self) -> bool {
        let doc = &self.session.doc;
        self.original.as_ref().is_some_and(|o| *o != doc.base || !doc.objects.is_empty() || !doc.images.is_empty())
    }

    /// The picture as first captured, with no annotations: one undo step, so Ctrl+Z brings the
    /// changes back.
    pub fn revert_to_original(&mut self) {
        let Some(original) = self.original.clone() else { return };
        if !self.can_revert() {
            return;
        }
        self.finish_edits();
        self.crop = None;
        let mut doc = Document::new(original);
        doc.meta = self.session.doc.meta.clone();
        self.session.begin("Revert to Original");
        self.session.doc = doc;
        self.session.selection.clear();
        self.session.commit();
        self.view.fit_pending = true;
    }

    /// The modal when a save something waited on failed: Save As…, let the changes go, or stay.
    pub(crate) fn save_problem_modal(&mut self, ctx: &egui::Context) {
        let Some(error) = self.save_problem.clone() else { return };
        // A Save As dialog is open: its answer decides.
        if self.saving_project {
            return;
        }
        let what = match &self.pending {
            Some(Pending::Quit) => "close the window",
            Some(_) => "open another picture",
            None => {
                self.save_problem = None;
                return;
            }
        };
        let mut choice: Option<u8> = None;
        egui::Modal::new(egui::Id::new("not-saved")).show(ctx, |ui| {
            ui.set_width(400.0);
            ui.heading("Your changes aren't saved");
            ui.add_space(4.0);
            ui.label(format!("“{}” couldn't be saved: {error}", self.name));
            ui.label(format!("Save it somewhere else, or {what} without the latest changes?"));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Save As…").clicked() {
                    choice = Some(0);
                }
                if ui.button("Discard Changes").clicked() {
                    choice = Some(1);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(2);
                }
            });
        });
        match choice {
            Some(0) => self.save_as(),
            Some(1) => {
                self.save_problem = None;
                self.session.dirty = false;
                self.saver.discard();
            }
            Some(2) => {
                self.save_problem = None;
                self.pending = None;
                self.saver.urgent = false;
            }
            _ => {}
        }
    }
}

/// `C:\…\capture.png` → `capture`.
pub(crate) fn stem(path: &str) -> String {
    std::path::Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_waits_for_a_quiet_second() {
        let mut s = Saver::default();
        s.watch(2, 10.0);
        assert!(!s.due(true, 10.5, false));
        assert_eq!(s.wait(true, 10.5, false), Some(0.5));
        assert!(s.due(true, 11.0, false));
        // Another change starts the second again.
        s.watch(3, 10.9);
        assert!(!s.due(true, 11.5, false));
        assert!(s.due(true, 11.9, false));
        assert!(!s.due(false, 20.0, false), "nothing to write");
    }

    #[test]
    fn nothing_is_written_mid_drag_unless_something_waits_on_it() {
        let mut s = Saver::default();
        s.watch(2, 0.0);
        assert!(!s.due(true, 5.0, true));
        assert_eq!(s.wait(true, 5.0, true), None);
        s.urgent = true;
        assert!(s.due(true, 0.0, true));
    }

    #[test]
    fn one_save_at_a_time_and_a_failure_waits_for_the_next_change() {
        let mut s = Saver::default();
        s.watch(2, 0.0);
        let id = s.start(2);
        assert!(s.writing() && !s.due(true, 9.0, false));
        assert_eq!(s.finish(&Saved { id: id + 1, result: Ok(()) }), None, "not the save under way");
        assert_eq!(s.finish(&Saved { id, result: Err("disk full".into()) }), Some((2, false)));
        assert_eq!(s.problem(true), Some("disk full"));
        assert!(!s.due(true, 9.0, false), "not tried again by itself");
        s.urgent = true;
        assert!(s.due(true, 9.0, false), "but when something waits on it");
        s.urgent = false;
        s.watch(3, 9.0);
        assert_eq!(s.problem(true), None);
        assert!(s.due(true, 10.0, false));
    }

    #[test]
    fn ctrl_s_writes_even_unchanged_and_says_so() {
        let mut s = Saver::default();
        assert!(s.settled(false));
        s.save_now();
        assert!(!s.settled(false) && s.due(false, 0.0, true));
        let id = s.start(1);
        assert_eq!(s.finish(&Saved { id, result: Ok(()) }), Some((1, true)));
        assert_eq!(s.status, Status::Saved);
        assert!(s.settled(false));
    }
}
