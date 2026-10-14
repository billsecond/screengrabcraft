//! The document being edited, with its selection and its undo and redo history.
//!
//! Every change goes through [`Session::execute`] (a command by id) or through an interactive
//! edit bracketed by [`Session::begin`] and [`Session::commit`] (a drag on the canvas). Either way
//! a snapshot of the document is kept before the change, so undo puts back exactly what was
//! there; the picture's pixels are shared between snapshots unless a command changed them.

use screengrabcraft_model::{Document, ObjectId};
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
}

impl Session {
    pub fn new(doc: Document) -> Session {
        Session { doc, selection: Vec::new(), undo: Vec::new(), redo: Vec::new(), dirty: false, revision: 1, pending: None }
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
}
