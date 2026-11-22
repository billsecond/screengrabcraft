//! The effects added after the first ones: colour changes that can be kept to an area (hue and
//! saturation, sepia, sharpen, blur), other edges (wavy, faded, a bevel), perspective, a page
//! curl, a reflection, a watermark and the capture's details. Their entries are in
//! [`crate::registry::COMMANDS`].

use screengrabcraft_model::{Color, FontName, Raster, RasterError, Rect};
use screengrabcraft_render::adjust::{self, Area};
use screengrabcraft_render::effects::{self, Edges};
use screengrabcraft_render::{frame, marks};
use serde_json::{Value, json};

use crate::registry::{boolean, color, num};
use crate::session::Session;
use crate::{EngineError, Result};

/// The most pictures a host lends a session at once.
pub const MAX_LENT: usize = 64;

/// A number given, or `default`.
fn or(p: &Value, key: &str, default: f32) -> f32 {
    num(p, key).unwrap_or(default)
}

/// The area an effect is kept to (`"area": {"x", "y", "w", "h"}`), in whole pixels on the
/// picture; none for all of it.
pub(crate) fn area(s: &Session, p: &Value) -> Result<Option<Area>> {
    let bad = || EngineError::Params("`area` is {\"x\", \"y\", \"w\", \"h\"} on the picture".to_string());
    match p.get("area") {
        None | Some(Value::Null) => Ok(None),
        Some(a @ Value::Object(_)) => {
            let n = |k: &str| num(a, k).ok_or_else(bad);
            let r = Rect::new(n("x")?, n("y")?, n("w")?, n("h")?).finite();
            r.pixel_bounds(s.doc.width(), s.doc.height()).map(Some).ok_or_else(bad)
        }
        Some(_) => Err(bad()),
    }
}

/// Grayscale, invert, brightness and the rest of the colour changes, on the area if one is given.
pub(crate) fn colours(s: &mut Session, p: &Value, f: impl FnOnce(&Raster) -> std::result::Result<Raster, RasterError>) -> Result<Value> {
    let a = area(s, p)?;
    s.doc.base = adjust::within(&s.doc.base, a, f)?;
    Ok(Value::Null)
}

pub(crate) fn hue_saturation(s: &mut Session, p: &Value) -> Result<Value> {
    let (h, sat, l, g) = (or(p, "hue", 0.0), or(p, "saturation", 0.0), or(p, "lightness", 0.0), or(p, "gamma", 1.0));
    colours(s, p, |r| Ok(adjust::hue_saturation(r, h, sat, l, g)))
}

pub(crate) fn sepia(s: &mut Session, p: &Value) -> Result<Value> {
    let amount = or(p, "amount", 100.0);
    colours(s, p, |r| Ok(adjust::sepia(r, amount)))
}

pub(crate) fn sharpen(s: &mut Session, p: &Value) -> Result<Value> {
    let (amount, radius) = (or(p, "amount", 60.0), or(p, "radius", 1.5));
    colours(s, p, |r| adjust::sharpen(r, amount, radius))
}

pub(crate) fn blur(s: &mut Session, p: &Value) -> Result<Value> {
    let radius = or(p, "radius", 4.0);
    colours(s, p, |r| adjust::blur(r, radius))
}

fn edges(p: &Value, all: bool) -> Edges {
    Edges { top: boolean(p, "top", all), right: boolean(p, "right", all), bottom: boolean(p, "bottom", true), left: boolean(p, "left", all) }
}

pub(crate) fn wavy(s: &mut Session, p: &Value) -> Result<Value> {
    s.doc.base = frame::wavy_edge(&s.doc.base, edges(p, false), or(p, "depth", 10.0), or(p, "length", 40.0))?;
    Ok(Value::Null)
}

pub(crate) fn faded(s: &mut Session, p: &Value) -> Result<Value> {
    s.doc.base = frame::faded_edge(&s.doc.base, edges(p, true), or(p, "width", 30.0));
    Ok(Value::Null)
}

pub(crate) fn bevel(s: &mut Session, p: &Value) -> Result<Value> {
    s.doc.base = frame::bevel(&s.doc.base, or(p, "width", 8.0), or(p, "strength", 50.0));
    Ok(Value::Null)
}

pub(crate) fn perspective(s: &mut Session, p: &Value) -> Result<Value> {
    let (pic, map) = frame::perspective(&s.doc.base, or(p, "horizontal", 30.0), or(p, "vertical", 0.0))?;
    s.doc.base = pic;
    // The annotations go where their places on the picture went.
    s.doc.map_objects(&|q| map.apply(q), 1.0);
    Ok(json!({ "width": s.doc.width(), "height": s.doc.height() }))
}

/// A named choice from a list (`default` when not given).
fn choice<T: serde::de::DeserializeOwned>(p: &Value, key: &str, default: T, what: &str) -> Result<T> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => serde_json::from_value(v.clone()).map_err(|_| EngineError::Params(format!("`{key}` is {what}"))),
    }
}

pub(crate) fn page_curl(s: &mut Session, p: &Value) -> Result<Value> {
    let corner = choice(p, "corner", frame::Corner::default(), "\"topLeft\", \"topRight\", \"bottomRight\" or \"bottomLeft\"")?;
    s.doc.base = frame::page_curl(&s.doc.base, corner, or(p, "size", 20.0))?;
    Ok(Value::Null)
}

pub(crate) fn reflection(s: &mut Session, p: &Value) -> Result<Value> {
    s.doc.base = frame::reflection(&s.doc.base, or(p, "height", 35.0), or(p, "opacity", 40.0), or(p, "gap", 0.0))?;
    Ok(json!({ "width": s.doc.width(), "height": s.doc.height() }))
}

/// Words from `key`, if they are given.
fn words(p: &Value, key: &str) -> Result<Option<String>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) => Ok(Some(t.chars().take(marks::MAX_MARK_TEXT).collect())),
        Some(_) => Err(EngineError::Params(format!("`{key}` is text"))),
    }
}

pub(crate) fn watermark(s: &mut Session, p: &Value) -> Result<Value> {
    let text = words(p, "text")?;
    let lent = match p.get("picture") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let i = v.as_u64().ok_or_else(|| EngineError::Params("`picture` is the number of a picture the host lent".to_string()))?;
            Some(
                s.lent
                    .get(usize::try_from(i).unwrap_or(usize::MAX))
                    .cloned()
                    .ok_or_else(|| EngineError::Failed(format!("there is no picture {i} to make a watermark of")))?,
            )
        }
    };
    let font = FontName::new(&words(p, "font")?.unwrap_or_default());
    let colour = color(p, "color", Color::WHITE)?;
    let mark = match (&text, &lent) {
        (_, Some(pic)) => marks::Mark::Picture(pic),
        (Some(t), None) if !t.trim().is_empty() => marks::Mark::Words { text: t, color: colour, bold: boolean(p, "bold", true), font },
        _ => return Err(EngineError::Params("give the watermark's `text`, or a `picture`".to_string())),
    };
    let wm = marks::Watermark {
        mark,
        place: choice(
            p,
            "place",
            marks::Place::Center,
            "\"center\", \"tiled\", \"topLeft\", \"top\", \"topRight\", \"left\", \"right\", \"bottomLeft\", \"bottom\" or \"bottomRight\"",
        )?,
        size: or(p, "size", 30.0),
        angle: or(p, "angle", 0.0),
        opacity: or(p, "opacity", 30.0) / 100.0,
        under: boolean(p, "under", false),
    };
    s.doc.base = marks::watermark(&s.doc.base, &wm)?;
    Ok(Value::Null)
}

/// What a capture detail says: the text given, or (when `true`) what the document knows.
fn detail(p: &Value, key: &str, known: Option<String>) -> Result<Option<String>> {
    match p.get(key) {
        None | Some(Value::Null) | Some(Value::Bool(true)) => Ok(known),
        Some(Value::Bool(false)) => Ok(None),
        Some(Value::String(t)) => Ok(Some(t.chars().take(120).collect())),
        Some(_) => Err(EngineError::Params(format!("`{key}` is true, false or the text to write"))),
    }
}

/// The words capture details write: the time, what it is of, its size, as asked.
pub fn details_text(s: &Session, p: &Value) -> Result<String> {
    let time = detail(p, "time", s.doc.meta.captured.as_deref().map(marks::readable_time))?;
    let program = detail(p, "program", s.doc.meta.source.clone())?;
    let size = boolean(p, "size", true).then(|| format!("{} × {} pixels", s.doc.width(), s.doc.height()));
    let parts: Vec<String> = [time, program, size].into_iter().flatten().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    Ok(parts.join("  ·  "))
}

pub(crate) fn capture_details(s: &mut Session, p: &Value) -> Result<Value> {
    let text = details_text(s, p)?;
    if text.is_empty() {
        return Err(EngineError::Failed(
            "there are no details to write: the time and what it is of aren't known, and the size is left out".to_string(),
        ));
    }
    let place = choice(p, "place", marks::DetailsPlace::default(), "\"bottomRight\", \"bottomLeft\", \"topRight\", \"topLeft\" or \"below\"")?;
    let fg = color(p, "color", Color::WHITE)?;
    let bg = color(p, "background", Color::rgba(0, 0, 0, 160))?;
    s.doc.base = marks::details(&s.doc.base, &text, place, or(p, "fontSize", 14.0), fg, bg)?;
    Ok(json!({ "text": text, "width": s.doc.width(), "height": s.doc.height() }))
}

/// Greyscale, invert and brightness kept to an area.
pub(crate) fn grayscale(s: &mut Session, p: &Value) -> Result<Value> {
    colours(s, p, |r| Ok(effects::grayscale(r)))
}

pub(crate) fn invert(s: &mut Session, p: &Value) -> Result<Value> {
    colours(s, p, |r| Ok(effects::invert(r)))
}

pub(crate) fn brightness(s: &mut Session, p: &Value) -> Result<Value> {
    let (b, c) = (or(p, "brightness", 0.0), or(p, "contrast", 0.0));
    colours(s, p, |r| Ok(effects::brightness_contrast(r, b, c)))
}
