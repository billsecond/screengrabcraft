//! The project file (`.sgcraft`): a picture with its annotations still editable.
//!
//! JSON, so it can be read and written by other programs and by agents: the picture and any
//! placed pictures as base64 PNG, the annotations as the model writes them.
//!
//! ```json
//! {"format":"screengrabcraft","version":2,"width":800,"height":600,
//!  "base":"iVBORw0…","images":{"1":"iVBORw0…"},
//!  "objects":[{"id":1,"type":"arrow","from":{"x":10,"y":10},"to":{"x":90,"y":40},"style":{…}}],
//!  "meta":{"captured":"2026-10-14T09:30:00","source":"Notepad"},
//!  "original":"iVBORw0…"}
//! ```
//!
//! Version 2 keeps the picture as first captured, so the owner can always go back to it:
//! `original` once the picture itself has been changed (cropped, resized…), or
//! `"baseIsOriginal":true` while `base` still is it. Neither means it isn't known. Version 1
//! files have neither, and a version 1 reader passes over both.
//!
//! A project file can come from anywhere: everything in it is checked (sizes, ids, numbers)
//! as it is read, and an annotation that doesn't make sense is left out rather than refusing
//! the file.

use std::collections::BTreeMap;

use base64::Engine;
use screengrabcraft_model::{Document, ImageFormat, Meta, Object, Raster};
use serde::{Deserialize, Serialize};

use crate::codec::{self, CodecError};

/// The format's name in the file.
pub const FORMAT: &str = "screengrabcraft";
/// The version written; files of later versions are read as far as they can be.
pub const VERSION: u32 = 3;
/// The first version that says what the original picture is.
pub const ORIGINAL_SINCE: u32 = 2;
/// Undo (and redo) steps kept in a file, at most each.
pub const MAX_HISTORY: usize = 30;
/// Pictures the history keeps besides the document's own (a crop or a resize makes one).
pub const MAX_HISTORY_PICTURES: usize = 6;
/// The largest project file read.
pub const MAX_PROJECT: usize = 1024 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("this is not a ScreenGrabCraft project: {0}")]
    NotAProject(String),
    #[error(transparent)]
    Picture(#[from] CodecError),
}

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    #[serde(default)]
    version: u32,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    base: String,
    #[serde(default)]
    images: BTreeMap<String, String>,
    /// Kept as raw JSON, so one bad annotation doesn't spoil the rest.
    #[serde(default)]
    objects: Vec<serde_json::Value>,
    #[serde(default)]
    meta: Meta,
    /// The picture as first captured, when `base` is no longer it. Raw JSON, so a bad one is
    /// left out rather than refusing the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original: Option<serde_json::Value>,
    /// `base` is still the picture as first captured.
    #[serde(default, rename = "baseIsOriginal", skip_serializing_if = "Option::is_none")]
    base_is_original: Option<serde_json::Value>,
    /// The undo and redo steps (version 3). Raw JSON, so a bad one is left out rather than
    /// refusing the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    history: Option<serde_json::Value>,
}

/// The history in the file: each step is the document as it was, its pictures by number in
/// `pictures` (`null`: the same as the document's own).
#[derive(Serialize, Deserialize, Default)]
struct HistoryFile {
    #[serde(default)]
    pictures: Vec<String>,
    #[serde(default)]
    undo: Vec<serde_json::Value>,
    #[serde(default)]
    redo: Vec<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
struct StepFile {
    #[serde(default)]
    label: String,
    #[serde(default)]
    base: Option<usize>,
    #[serde(default)]
    images: BTreeMap<String, Option<usize>>,
    #[serde(default)]
    objects: Vec<serde_json::Value>,
    #[serde(default)]
    meta: Meta,
}

/// One undo or redo step: what it was called, and the document as it was.
#[derive(Clone, Debug)]
pub struct Step {
    pub label: String,
    pub doc: Document,
}

/// The steps Undo and Redo go through, in the session's order (the next to undo or redo last).
#[derive(Clone, Debug, Default)]
pub struct History {
    pub undo: Vec<Step>,
    pub redo: Vec<Step>,
}

/// A project file as read.
#[derive(Clone, Debug)]
pub struct Project {
    pub doc: Document,
    /// The picture as first captured, if the file knows it.
    pub original: Option<Raster>,
    /// Whether the file is of a version that says what the original is. An older one doesn't,
    /// and the picture it was saved beside is still the original.
    pub tells_original: bool,
    /// Annotations that couldn't be read and were left out.
    pub skipped: usize,
    /// The undo and redo steps saved with it (version 3 files).
    pub history: History,
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unb64(text: &str) -> Result<Vec<u8>, ProjectError> {
    base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .map_err(|e| ProjectError::NotAProject(format!("a picture in it is not base64: {e}")))
}

fn png(r: &Raster) -> Result<String, ProjectError> {
    Ok(b64(&codec::encode(r, ImageFormat::Png, 100)?))
}

/// The document as project-file bytes (the original picture not known).
pub fn save(doc: &Document) -> Result<Vec<u8>, ProjectError> {
    save_with_original(doc, None)
}

/// The document as project-file bytes, with the picture as first captured.
pub fn save_with_original(doc: &Document, original: Option<&Raster>) -> Result<Vec<u8>, ProjectError> {
    save_full(doc, original, None)
}

/// A picture of a step by number in `table`, or none when it's `current` (the document's own).
fn picture_ref(r: &Raster, current: Option<&Raster>, table: &mut Vec<Raster>) -> Option<usize> {
    let same = |a: &Raster, b: &Raster| a.same_pixels(b) || a == b;
    if current.is_some_and(|c| same(c, r)) {
        return None;
    }
    if let Some(i) = table.iter().position(|t| same(t, r)) {
        return Some(i);
    }
    table.push(r.clone());
    Some(table.len() - 1)
}

/// Steps as the file keeps them, newest kept first when there's no room for them all.
fn steps_out(steps: &[Step], doc: &Document, table: &mut Vec<Raster>) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for step in steps.iter().rev().take(MAX_HISTORY) {
        let before = table.len();
        let base = picture_ref(&step.doc.base, Some(&doc.base), table);
        let images: BTreeMap<String, Option<usize>> =
            step.doc.images.iter().map(|(k, r)| (k.to_string(), picture_ref(r, doc.images.get(k), table))).collect();
        // A step that would need more pictures than there's room for ends the history there.
        if table.len() > MAX_HISTORY_PICTURES {
            table.truncate(before);
            break;
        }
        let objects = step.doc.objects.iter().filter_map(|o| serde_json::to_value(o).ok()).collect();
        let file = StepFile { label: step.label.clone(), base, images, objects, meta: step.doc.meta.clone() };
        match serde_json::to_value(file) {
            Ok(v) => out.push(v),
            Err(_) => break,
        }
    }
    out.reverse();
    out
}

/// The document as project-file bytes, with the picture as first captured and the undo and
/// redo steps (the newest [`MAX_HISTORY`] of each, as far as [`MAX_HISTORY_PICTURES`] allows).
pub fn save_full(doc: &Document, original: Option<&Raster>, history: Option<&History>) -> Result<Vec<u8>, ProjectError> {
    let history = match history {
        Some(h) if !h.undo.is_empty() || !h.redo.is_empty() => {
            let mut table = Vec::new();
            let undo = steps_out(&h.undo, doc, &mut table);
            let redo = steps_out(&h.redo, doc, &mut table);
            let pictures = table.iter().map(png).collect::<Result<Vec<String>, ProjectError>>()?;
            serde_json::to_value(HistoryFile { pictures, undo, redo }).ok()
        }
        _ => None,
    };
    let mut images = BTreeMap::new();
    for (k, r) in &doc.images {
        images.insert(k.to_string(), png(r)?);
    }
    let objects = doc.objects.iter().filter_map(|o| serde_json::to_value(o).ok()).collect();
    let same = original.is_some_and(|o| *o == doc.base);
    let base_is_original = same.then_some(serde_json::Value::Bool(true));
    let original = match original {
        Some(o) if !same => Some(serde_json::Value::String(png(o)?)),
        _ => None,
    };
    let file = File {
        format: FORMAT.to_string(),
        version: VERSION,
        width: doc.width(),
        height: doc.height(),
        base: png(&doc.base)?,
        images,
        objects,
        meta: doc.meta.clone(),
        original,
        base_is_original,
        history,
    };
    serde_json::to_vec(&file).map_err(|e| ProjectError::NotAProject(e.to_string()))
}

/// The history of a file, checked: a picture that doesn't read leaves out the steps that need
/// it, and a step that doesn't read ends the history there (older steps would skip over it).
fn history_in(value: Option<serde_json::Value>, doc: &Document) -> History {
    let Some(value) = value else { return History::default() };
    let Ok(file) = serde_json::from_value::<HistoryFile>(value) else {
        log::warn!("the undo history in the file could not be read");
        return History::default();
    };
    let pictures: Vec<Option<Raster>> =
        file.pictures.iter().take(MAX_HISTORY_PICTURES).map(|p| unb64(p).ok().and_then(|b| codec::decode(&b).ok())).collect();
    let picture = |i: Option<usize>, current: Option<&Raster>| -> Option<Raster> {
        match i {
            None => current.cloned(),
            Some(i) => pictures.get(i).cloned().flatten(),
        }
    };
    let step = |v: serde_json::Value| -> Option<Step> {
        let s: StepFile = serde_json::from_value(v).ok()?;
        let mut d = Document::new(picture(s.base, Some(&doc.base))?);
        d.meta = s.meta;
        for (k, i) in s.images.iter().take(screengrabcraft_model::document::MAX_IMAGES) {
            let key = k.parse::<u64>().ok()?;
            d.images.insert(key, picture(*i, doc.images.get(&key))?);
        }
        for o in s.objects {
            d.insert(serde_json::from_value::<Object>(o).ok()?).ok()?;
        }
        Some(Step { label: s.label.chars().take(80).collect(), doc: d })
    };
    // Undo: newest last, so the reading goes from the newest back and stops at a bad one.
    let mut undo: Vec<Step> = file.undo.into_iter().rev().take(MAX_HISTORY).map_while(step).collect();
    undo.reverse();
    let mut redo: Vec<Step> = file.redo.into_iter().rev().take(MAX_HISTORY).map_while(step).collect();
    redo.reverse();
    History { undo, redo }
}

/// Reads a project file. Annotations that can't be read are skipped (and counted in the
/// returned number).
pub fn load(bytes: &[u8]) -> Result<(Document, usize), ProjectError> {
    read(bytes).map(|p| (p.doc, p.skipped))
}

/// Reads a project file with what it knows of the original picture. An original that can't be
/// read is left out (the document still opens).
pub fn read(bytes: &[u8]) -> Result<Project, ProjectError> {
    if bytes.len() > MAX_PROJECT {
        return Err(ProjectError::NotAProject("the file is too large".to_string()));
    }
    let file: File = serde_json::from_slice(bytes).map_err(|e| ProjectError::NotAProject(e.to_string()))?;
    if file.format != FORMAT {
        return Err(ProjectError::NotAProject(format!("its format is {:?}", file.format)));
    }
    if file.version > VERSION {
        log::warn!("project file version {} is newer than {VERSION}; reading what this version knows", file.version);
    }
    let base = codec::decode(&unb64(&file.base)?)?;
    if (file.width, file.height) != (0, 0) && (file.width, file.height) != (base.width(), base.height()) {
        log::warn!("project says {}x{} but its picture is {}x{}", file.width, file.height, base.width(), base.height());
    }
    let original = match &file.original {
        Some(data) => {
            let text = data.as_str().ok_or_else(|| "it is not text".to_string());
            match text.and_then(|t| unb64(t).map_err(|e| e.to_string())).and_then(|b| codec::decode(&b).map_err(|e| e.to_string())) {
                Ok(r) => Some(r),
                Err(e) => {
                    log::warn!("the original picture could not be read: {e}");
                    None
                }
            }
        }
        None if file.base_is_original.as_ref().and_then(serde_json::Value::as_bool) == Some(true) => Some(base.clone()),
        None => None,
    };
    let tells_original = file.version >= ORIGINAL_SINCE;
    let mut doc = Document::new(base);
    doc.meta = file.meta;
    // Placed pictures keep their keys, so the annotations' references still hold.
    for (k, data) in file.images.iter().take(screengrabcraft_model::document::MAX_IMAGES) {
        let Ok(key) = k.parse::<u64>() else { continue };
        match unb64(data).map_err(|e| e.to_string()).and_then(|b| codec::decode(&b).map_err(|e| e.to_string())) {
            Ok(r) => {
                doc.images.insert(key, r);
            }
            Err(e) => log::warn!("placed picture {key} could not be read: {e}"),
        }
    }
    let mut skipped = 0;
    for v in file.objects {
        match serde_json::from_value::<Object>(v) {
            Ok(o) => {
                if doc.insert(o).is_err() {
                    skipped += 1;
                }
            }
            Err(e) => {
                log::warn!("an annotation could not be read: {e}");
                skipped += 1;
            }
        }
    }
    doc.prune_images();
    let history = history_in(file.history, &doc);
    Ok(Project { doc, original, tells_original, skipped, history })
}

/// Whether bytes look like a project file (rather than a picture).
pub fn sniff(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(256)];
    let text = String::from_utf8_lossy(head);
    let t = text.trim_start_matches('\u{feff}').trim_start();
    t.starts_with('{') && t.contains(FORMAT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::{Color, Kind, Pt, Rect, Shape};

    fn doc() -> Document {
        let mut d = Document::new(Raster::filled(30, 20, Color::WHITE).unwrap());
        d.add(Shape::Arrow { from: Pt::new(1.0, 1.0), to: Pt::new(20.0, 10.0), heads: Default::default() }, Kind::Arrow.default_style()).unwrap();
        let key = d.add_image(Raster::filled(3, 3, Color::RED).unwrap()).unwrap();
        d.add(Shape::Image { rect: Rect::new(5.0, 5.0, 3.0, 3.0), image: key }, Kind::Image.default_style()).unwrap();
        d.add(Shape::Text { rect: Rect::new(0.0, 0.0, 20.0, 10.0), text: "hi ✓".into() }, Kind::Text.default_style()).unwrap();
        d.meta.source = Some("Notepad".into());
        d
    }

    #[test]
    fn round_trips() {
        let d = doc();
        let bytes = save(&d).unwrap();
        assert!(sniff(&bytes));
        let (back, skipped) = load(&bytes).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(back.base, d.base);
        assert_eq!(back.objects, d.objects);
        assert_eq!(back.images, d.images);
        assert_eq!(back.meta, d.meta);
    }

    #[test]
    fn bad_parts_are_skipped_and_bad_files_refused() {
        let mut v: serde_json::Value = serde_json::from_slice(&save(&doc()).unwrap()).unwrap();
        v["objects"].as_array_mut().unwrap().push(serde_json::json!({"id": 9, "type": "teapot"}));
        v["objects"].as_array_mut().unwrap().push(serde_json::json!({"id": 10, "type": "image", "rect": {"x":0,"y":0,"w":1,"h":1}, "image": 77}));
        v["objects"].as_array_mut().unwrap().push(serde_json::json!({"id": 11, "type": "line", "from": {"x": 1e30, "y": 0}, "to": {"x": 0, "y": 0}}));
        let (d, skipped) = load(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(skipped, 2);
        assert_eq!(d.objects.len(), 4);
        assert!(load(b"{}").is_err());
        assert!(load(b"not json").is_err());
        assert!(load(br#"{"format":"other","base":""}"#).is_err());
        assert!(load(br#"{"format":"screengrabcraft","base":"!!!"}"#).is_err());
        assert!(!sniff(b"\x89PNG"));
    }

    #[test]
    fn the_original_picture_is_kept_beside_a_changed_one() {
        let mut d = doc();
        let original = Raster::filled(60, 40, Color::RED).unwrap();
        d.base = Raster::filled(30, 20, Color::WHITE).unwrap();
        let bytes = save_with_original(&d, Some(&original)).unwrap();
        let p = read(&bytes).unwrap();
        assert!(p.tells_original);
        assert_eq!(p.original, Some(original));
        assert_eq!(p.doc.base, d.base);
        assert_eq!(p.doc.objects, d.objects);
    }

    #[test]
    fn an_unchanged_picture_is_written_once() {
        let d = doc();
        let once = save(&d).unwrap();
        let bytes = save_with_original(&d, Some(&d.base)).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["baseIsOriginal"], serde_json::json!(true));
        assert!(v.get("original").is_none());
        assert!(bytes.len() < once.len() + 40, "no second copy of the picture");
        assert_eq!(read(&bytes).unwrap().original, Some(d.base.clone()));
        // Not known: neither is written, and a reader knows it isn't.
        let p = read(&once).unwrap();
        assert!(p.tells_original && p.original.is_none());
    }

    #[test]
    fn a_version_1_file_still_opens() {
        let v1 = br#"{"format":"screengrabcraft","version":1,"width":30,"height":20,"base":"BASE","objects":[]}"#;
        let base = b64(&codec::encode(&Raster::filled(30, 20, Color::WHITE).unwrap(), ImageFormat::Png, 100).unwrap());
        let text = String::from_utf8_lossy(v1).replace("BASE", &base);
        let p = read(text.as_bytes()).unwrap();
        assert_eq!((p.doc.width(), p.doc.height()), (30, 20));
        assert!(!p.tells_original, "a version 1 file says nothing of the original");
        assert!(p.original.is_none());
    }

    #[test]
    fn a_version_1_reader_reads_a_version_2_file() {
        // The file as version 1 read it: no `original`, no `baseIsOriginal`, nothing refused.
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct V1 {
            format: String,
            #[serde(default)]
            version: u32,
            base: String,
            #[serde(default)]
            images: BTreeMap<String, String>,
            #[serde(default)]
            objects: Vec<serde_json::Value>,
            #[serde(default)]
            meta: Meta,
        }
        let mut d = doc();
        let original = Raster::filled(60, 40, Color::RED).unwrap();
        d.base = Raster::filled(30, 20, Color::WHITE).unwrap();
        let old: V1 = serde_json::from_slice(&save_with_original(&d, Some(&original)).unwrap()).unwrap();
        assert_eq!(old.objects.len(), d.objects.len());
        assert_eq!(codec::decode(&unb64(&old.base).unwrap()).unwrap(), d.base);
    }

    #[test]
    fn a_bad_original_is_left_out_and_the_rest_still_opens() {
        let mut v: serde_json::Value = serde_json::from_slice(&save(&doc()).unwrap()).unwrap();
        v["original"] = serde_json::json!("!!! not a picture");
        let p = read(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert!(p.original.is_none());
        assert_eq!(p.doc.objects.len(), 3);
        v["original"] = serde_json::json!(42);
        v["baseIsOriginal"] = serde_json::json!("yes");
        let p = read(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert!(p.original.is_none());
        assert_eq!(p.doc.objects.len(), 3);
    }

    /// A history of `n` steps, each the document with one annotation fewer, and a crop first.
    fn history(n: usize) -> (Document, History) {
        let mut d = Document::new(Raster::filled(40, 30, Color::WHITE).unwrap());
        let mut undo = vec![Step { label: "Crop".into(), doc: Document::new(Raster::filled(80, 60, Color::WHITE).unwrap()) }];
        for i in 0..n {
            undo.push(Step { label: format!("Draw {i}"), doc: d.clone() });
            d.add(
                Shape::Arrow { from: Pt::new(1.0, 1.0), to: Pt::new(10.0 + i as f32, 10.0), heads: Default::default() },
                Kind::Arrow.default_style(),
            )
            .unwrap();
        }
        let redo = vec![Step { label: "Draw again".into(), doc: d.clone() }];
        (d, History { undo, redo })
    }

    #[test]
    fn the_undo_history_comes_back_with_the_pictures_it_needs() {
        let (d, h) = history(3);
        let bytes = save_full(&d, None, Some(&h)).unwrap();
        let p = read(&bytes).unwrap();
        let labels: Vec<&str> = p.history.undo.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Crop", "Draw 0", "Draw 1", "Draw 2"]);
        // The crop's step has its own picture; the others share the document's.
        assert_eq!(p.history.undo[0].doc.width(), 80);
        assert_eq!(p.history.undo[3].doc.objects.len(), 2);
        assert_eq!(p.history.redo.len(), 1);
        // An old reader's file (no history) opens with none.
        assert!(read(&save(&d).unwrap()).unwrap().history.undo.is_empty());
    }

    #[test]
    fn the_history_keeps_the_newest_steps_when_there_are_too_many() {
        let (d, h) = history(MAX_HISTORY + 10);
        let p = read(&save_full(&d, None, Some(&h)).unwrap()).unwrap();
        assert_eq!(p.history.undo.len(), MAX_HISTORY);
        assert_eq!(p.history.undo.last().map(|s| s.label.clone()), Some(format!("Draw {}", MAX_HISTORY + 9)));
    }

    #[test]
    fn a_damaged_step_ends_the_history_there_and_never_the_file() {
        let (d, h) = history(4);
        let mut v: serde_json::Value = serde_json::from_slice(&save_full(&d, None, Some(&h)).unwrap()).unwrap();
        // The third newest step's annotations made nonsense.
        v["history"]["undo"][2]["objects"] = serde_json::json!([{"type": "arrow", "nonsense": true}]);
        v["history"]["pictures"] = serde_json::json!(["not a picture"]);
        let p = read(&serde_json::to_vec(&v).unwrap()).unwrap();
        let labels: Vec<&str> = p.history.undo.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Draw 2", "Draw 3"]);
        assert_eq!(p.doc.objects.len(), 4);
        // History that isn't a history at all: the document still opens.
        v["history"] = serde_json::json!("junk");
        assert!(read(&serde_json::to_vec(&v).unwrap()).unwrap().history.undo.is_empty());
    }
}
