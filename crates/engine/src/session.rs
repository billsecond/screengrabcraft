//! The document being edited, with its selection and its undo and redo history.
//!
//! Every change goes through [`Session::execute`] (a command by id) or through an interactive
//! edit bracketed by [`Session::begin`] and [`Session::commit`] (a drag on the canvas). Either way
//! a snapshot of the document is kept before the change, so undo puts back exactly what was
//! there; the picture's pixels are shared between snapshots unless a command changed them.

use std::sync::Arc;

use screengrabcraft_model::{Document, ObjectId, Raster};
use screengrabcraft_render::matte::{Matte, Matting};
use serde_json::Value;

use crate::EngineError;
use crate::registry;

/// How many steps undo remembers.
pub const MAX_UNDO: usize = 200;

#[derive(Clone, Debug)]
struct Snapshot {
    doc: Document,
    selection: Vec<ObjectId>,
    label: String,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub doc: Document,
    pub selection: Vec<ObjectId>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Changed since it was last saved.
    pub dirty: bool,
    revision: u64,
    pending: Option<Snapshot>,
    /// What `image.removeBackground` works with. It isn't part of the document or its history.
    pub background: Background,
}

/// What `image.removeBackground` works with: a model the host lent the session, and a matte
/// worked out ahead of time for the picture as it was then.
///
/// A model takes seconds, so the editor works the matte out on a thread of its own and hands it
/// over ([`Background::prepare`]) before running the command, which then only applies it. With
/// no matte ready, the command runs the model in place (the command-line program, scripts).
#[derive(Clone, Default)]
pub struct Background {
    model: Option<Arc<dyn Matting>>,
    prepared: Option<(Raster, String, Matte)>,
}

impl std::fmt::Debug for Background {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Background").field("model", &self.model.as_ref().map(|m| m.name())).field("prepared", &self.prepared.is_some()).finish()
    }
}

impl Background {
    /// Lends a model, or takes it back.
    pub fn set_model(&mut self, model: Option<Arc<dyn Matting>>) {
        self.model = model;
    }

    pub fn model(&self) -> Option<&Arc<dyn Matting>> {
        self.model.as_ref()
    }

    /// Hands over the matte `model` worked out for `picture`. It is used by the next
    /// `image.removeBackground` if the picture is still that one.
    pub fn prepare(&mut self, picture: Raster, model: &str, matte: Matte) {
        self.prepared = Some((picture, model.to_string(), matte));
    }

    /// The matte prepared for `picture` (by `model`, if one is named), taken.
    pub(crate) fn take_prepared(&mut self, picture: &Raster, model: Option<&str>) -> Option<(String, Matte)> {
        let (for_picture, name, matte) = self.prepared.take()?;
        let same = for_picture.same_pixels(picture) && (for_picture.width(), for_picture.height()) == (picture.width(), picture.height());
        (same && model.is_none_or(|m| m == name)).then_some((name, matte))
    }
}

impl Session {
    pub fn new(doc: Document) -> Session {
        Session {
            doc,
            selection: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
            revision: 1,
            pending: None,
            background: Background::default(),
        }
    }

    /// Counts up whenever the document changes (the editor draws the picture again when it
    /// moves; a change of selection alone doesn't move it).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Notes that the document changed (for interactive edits in progress).
    pub fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot { doc: self.doc.clone(), selection: self.selection.clone(), label: label.to_string() }
    }

    fn push_undo(&mut self, snap: Snapshot) {
        self.undo.push(snap);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.touch();
    }

    /// Runs a command by id with JSON parameters. A command that fails (or panics) leaves the
    /// document as it was.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value, EngineError> {
        let spec = registry::find(id).ok_or_else(|| EngineError::Unknown(id.to_string()))?;
        if !(spec.enabled)(self) {
            return Err(EngineError::Disabled(id.to_string()));
        }
        if !spec.undoable {
            return (spec.run)(self, params);
        }
        let before = self.snapshot(spec.label);
        let run = spec.run;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(self, params)));
        match result {
            Ok(Ok(out)) => {
                if self.doc != before.doc {
                    self.push_undo(before);
                }
                Ok(out)
            }
            Ok(Err(e)) => {
                self.doc = before.doc;
                self.selection = before.selection;
                Err(e)
            }
            Err(_) => {
                log::error!("command {id} panicked; the document was put back");
                self.doc = before.doc;
                self.selection = before.selection;
                Err(EngineError::Panicked(id.to_string()))
            }
        }
    }

    /// Starts an interactive edit (a drag): the document as it is now is kept for undo.
    pub fn begin(&mut self, label: &str) {
        if self.pending.is_none() {
            self.pending = Some(self.snapshot(label));
        }
    }

    /// Whether an interactive edit is under way.
    pub fn editing(&self) -> bool {
        self.pending.is_some()
    }

    /// Ends an interactive edit; it is one undo step if it changed anything.
    pub fn commit(&mut self) {
        if let Some(before) = self.pending.take()
            && before.doc != self.doc
        {
            self.push_undo(before);
        }
    }

    /// Ends an interactive edit by putting the document back as it was.
    pub fn cancel(&mut self) {
        if let Some(before) = self.pending.take() {
            self.doc = before.doc;
            self.selection = before.selection;
            self.touch();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// What undo would take back.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    pub fn undo(&mut self) -> bool {
        self.commit();
        let Some(prev) = self.undo.pop() else { return false };
        let now = Snapshot {
            doc: std::mem::replace(&mut self.doc, prev.doc),
            selection: std::mem::replace(&mut self.selection, prev.selection),
            label: prev.label.clone(),
        };
        self.redo.push(now);
        self.dirty = true;
        self.touch();
        true
    }

    pub fn redo(&mut self) -> bool {
        self.commit();
        let Some(next) = self.redo.pop() else { return false };
        let now = Snapshot {
            doc: std::mem::replace(&mut self.doc, next.doc),
            selection: std::mem::replace(&mut self.selection, next.selection),
            label: next.label.clone(),
        };
        self.undo.push(now);
        self.dirty = true;
        self.touch();
        true
    }

    /// Replaces the whole document (a new capture or an opened file): history starts over.
    pub fn replace(&mut self, doc: Document) {
        self.background.prepared = None;
        self.doc = doc;
        self.selection.clear();
        self.undo.clear();
        self.redo.clear();
        self.pending = None;
        self.dirty = false;
        self.touch();
    }

    /// The selection, keeping only ids that still exist.
    pub fn selected(&self) -> Vec<ObjectId> {
        self.selection.iter().copied().filter(|id| self.doc.get(*id).is_some()).collect()
    }

    pub fn select(&mut self, ids: Vec<ObjectId>) {
        self.selection = ids;
    }

    /// The undo and redo steps, to keep in a file (the next to undo or redo last in each).
    pub fn history(&self) -> screengrabcraft_render::project::History {
        let step = |s: &Snapshot| screengrabcraft_render::project::Step { label: s.label.clone(), doc: s.doc.clone() };
        screengrabcraft_render::project::History { undo: self.undo.iter().map(step).collect(), redo: self.redo.iter().map(step).collect() }
    }

    /// Takes back the steps a file kept, after [`Session::replace`] put its document in place:
    /// Undo then goes back through what was done before the file was last closed.
    pub fn restore_history(&mut self, history: screengrabcraft_render::project::History) {
        let snap = |s: screengrabcraft_render::project::Step| Snapshot { doc: s.doc, selection: Vec::new(), label: s.label };
        self.undo = history.undo.into_iter().map(snap).collect();
        self.redo = history.redo.into_iter().map(snap).collect();
        let over = self.undo.len().saturating_sub(MAX_UNDO);
        self.undo.drain(..over);
        self.redo.truncate(MAX_UNDO);
        self.touch();
    }
}
