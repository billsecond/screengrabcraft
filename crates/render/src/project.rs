//! The project file (`.sgcraft`): a picture with its annotations still editable.
//!
//! JSON, so it can be read and written by other programs and by agents: the picture and any
//! placed pictures as base64 PNG, the annotations as the model writes them.
//!
//! ```json
//! {"format":"screengrabcraft","version":1,"width":800,"height":600,
//!  "base":"iVBORw0…","images":{"1":"iVBORw0…"},
//!  "objects":[{"id":1,"type":"arrow","from":{"x":10,"y":10},"to":{"x":90,"y":40},"style":{…}}],
//!  "meta":{"captured":"2026-10-14T09:30:00","source":"Notepad"}}
//! ```
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
pub const VERSION: u32 = 1;
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

/// The document as project-file bytes.
pub fn save(doc: &Document) -> Result<Vec<u8>, ProjectError> {
    let mut images = BTreeMap::new();
    for (k, r) in &doc.images {
        images.insert(k.to_string(), png(r)?);
    }
    let objects = doc.objects.iter().filter_map(|o| serde_json::to_value(o).ok()).collect();
    let file = File {
        format: FORMAT.to_string(),
        version: VERSION,
        width: doc.width(),
        height: doc.height(),
        base: png(&doc.base)?,
        images,
        objects,
        meta: doc.meta.clone(),
    };
    serde_json::to_vec(&file).map_err(|e| ProjectError::NotAProject(e.to_string()))
}

/// Reads a project file. Annotations that can't be read are skipped (and counted in the
/// returned number).
pub fn load(bytes: &[u8]) -> Result<(Document, usize), ProjectError> {
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
    Ok((doc, skipped))
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
}
