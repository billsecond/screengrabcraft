//! Every editing action as a command: an id, a label, where it belongs, its shortcut, what its
//! parameters are, and the function that does it. The editor's menus, toolbar and keyboard, the
//! command-line program and scripts all run commands by id, so they all behave the same.
//!
//! Parameters are JSON and can come from anywhere (a script, the command line): every one is
//! checked, and a command given nonsense returns an error and changes nothing.

use screengrabcraft_model::{Color, Document, Object, ObjectId, Order, Pt, Rect, Shape, Style};
use screengrabcraft_render::effects;
use serde_json::{Value, json};

use crate::session::Session;
use crate::{EngineError, Result};

/// A command.
pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// The menu it is in.
    pub group: &'static str,
    /// The keyboard shortcut shown next to it (handled by the editor).
    pub shortcut: Option<&'static str>,
    /// What parameters it takes (JSON), for people and agents.
    pub params: &'static str,
    /// Whether it is recorded for undo (undo and redo themselves are not).
    pub undoable: bool,
    pub enabled: fn(&Session) -> bool,
    pub run: fn(&mut Session, &Value) -> Result<Value>,
}

fn always(_: &Session) -> bool {
    true
}

fn has_selection(s: &Session) -> bool {
    !s.selected().is_empty()
}

fn has_objects(s: &Session) -> bool {
    !s.doc.objects.is_empty()
}

macro_rules! cmd {
    ($id:expr, $label:expr, $group:expr, $shortcut:expr, $params:expr, $enabled:expr, $run:expr) => {
        CommandSpec { id: $id, label: $label, group: $group, shortcut: $shortcut, params: $params, undoable: true, enabled: $enabled, run: $run }
    };
}

/// Every command.
pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "edit.undo",
        label: "Undo",
        group: "Edit",
        shortcut: Some("Ctrl+Z"),
        params: "{}",
        undoable: false,
        enabled: |s| s.can_undo(),
        run: |s, _| Ok(json!(s.undo())),
    },
    CommandSpec {
        id: "edit.redo",
        label: "Redo",
        group: "Edit",
        shortcut: Some("Ctrl+Y"),
        params: "{}",
        undoable: false,
        enabled: |s| s.can_redo(),
        run: |s, _| Ok(json!(s.redo())),
    },
    cmd!("edit.delete", "Delete", "Edit", Some("Delete"), r#"{"ids"?: [id]} (default: the selection)"#, has_selection, delete),
    cmd!("edit.selectAll", "Select All", "Edit", Some("Ctrl+A"), "{}", has_objects, |s, _| {
        let all = s.doc.objects.iter().map(|o| o.id).collect();
        s.select(all);
        Ok(Value::Null)
    }),
    cmd!("edit.selectNone", "Select None", "Edit", Some("Esc"), "{}", always, |s, _| {
        s.select(Vec::new());
        Ok(Value::Null)
    }),
    cmd!("edit.select", "Select", "Edit", None, r#"{"ids": [id]}"#, always, |s, p| {
        let ids = ids_param(s, p, false)?;
        s.select(ids);
        Ok(Value::Null)
    }),
    cmd!("edit.duplicate", "Duplicate", "Edit", Some("Ctrl+D"), r#"{"ids"?: [id], "dx"?: 16, "dy"?: 16}"#, has_selection, duplicate),
    cmd!("edit.nudge", "Nudge", "Edit", Some("Arrow keys"), r#"{"dx": 1, "dy": 0, "ids"?: [id]}"#, has_selection, nudge),
    cmd!(
        "object.add",
        "Add Annotation",
        "Annotate",
        None,
        r##"{"object": {"type": "arrow"|"line"|"rect"|"ellipse"|"pen"|"highlight"|"text"|"callout"|"step"|"blur"|"pixelate"|"spotlight"|"magnify"|"stamp", …its fields, "style"?: {…}}} → {"id": id}. Shapes are in picture pixels: arrow/line {"from":{"x","y"},"to":{"x","y"}}, boxes {"rect":{"x","y","w","h"}}, pen/highlight {"points":[…]}, text/callout {"text": "…"} (callout also "tail"), step {"center", "number"?}, stamp {"stamp": "check"|"cross"|"star"|"question"|"warning"|"info"|"heart"}"##,
        always,
        add_object
    ),
    cmd!(
        "object.update",
        "Change Annotation",
        "Annotate",
        None,
        r#"{"id": id, "object": {…shape fields}, "style"?: {…}}"#,
        has_objects,
        update_object
    ),
    cmd!(
        "object.style",
        "Change Style",
        "Annotate",
        None,
        r##"{"ids"?: [id], "style": {"stroke"?: "#RRGGBB", "fill"?, "width"?, "opacity"?, "shadow"?, "fontSize"?, "bold"?, …}}"##,
        has_selection,
        restyle
    ),
    cmd!("object.text", "Change Text", "Annotate", None, r#"{"id": id, "text": "…"}"#, has_objects, set_text),
    cmd!("object.order", "Arrange", "Annotate", None, r#"{"to": "front"|"back"|"forward"|"backward", "ids"?: [id]}"#, has_selection, order),
    cmd!(
        "object.flatten",
        "Flatten",
        "Annotate",
        Some("Ctrl+Shift+F"),
        "{} — draws every annotation into the picture for good",
        has_objects,
        flatten
    ),
    cmd!("image.crop", "Crop", "Image", None, r#"{"x": 0, "y": 0, "w": 100, "h": 100}"#, always, crop),
    cmd!(
        "image.resize",
        "Resize Image",
        "Image",
        None,
        r#"{"width"?: px, "height"?: px, "percent"?: 50} (one side alone keeps the proportions)"#,
        always,
        resize
    ),
    cmd!(
        "image.canvas",
        "Canvas Size",
        "Image",
        None,
        r##"{"left"?: 0, "top"?: 0, "right"?: 0, "bottom"?: 0, "color"?: "#FFFFFF00"}"##,
        always,
        canvas
    ),
    cmd!("image.rotate", "Rotate", "Image", None, r#"{"degrees": 90|180|270}"#, always, rotate),
    cmd!("image.flip", "Flip", "Image", None, r#"{"horizontal": true}"#, always, flip),
    cmd!("image.trim", "Trim Borders", "Image", None, r#"{"tolerance"?: 8}"#, always, trim),
    cmd!("effect.border", "Border", "Effects", None, r##"{"width"?: 4, "color"?: "#000000"}"##, always, border),
    cmd!("effect.shadow", "Drop Shadow", "Effects", None, r##"{"offset"?: 8, "softness"?: 10, "color"?: "#00000080"}"##, always, shadow),
    cmd!(
        "effect.tornEdge",
        "Torn Edge",
        "Effects",
        None,
        r#"{"top"?: false, "right"?: false, "bottom"?: true, "left"?: false, "depth"?: 14}"#,
        always,
        torn
    ),
    cmd!("effect.roundCorners", "Round Corners", "Effects", None, r#"{"radius"?: 16}"#, always, round),
    cmd!("effect.grayscale", "Grayscale", "Effects", None, "{}", always, |s, _| {
        s.doc.base = effects::grayscale(&s.doc.base);
        Ok(Value::Null)
    }),
    cmd!("effect.invert", "Invert Colors", "Effects", None, "{}", always, |s, _| {
        s.doc.base = effects::invert(&s.doc.base);
        Ok(Value::Null)
    }),
    cmd!("effect.brightness", "Brightness and Contrast", "Effects", None, r#"{"brightness"?: 0, "contrast"?: 0} (-100 to 100)"#, always, |s, p| {
        let b = num(p, "brightness").unwrap_or(0.0);
        let c = num(p, "contrast").unwrap_or(0.0);
        s.doc.base = effects::brightness_contrast(&s.doc.base, b, c);
        Ok(Value::Null)
    }),
    CommandSpec {
        id: "doc.info",
        label: "Document Information",
        group: "Document",
        shortcut: None,
        params: "{} → size, annotations and selection",
        undoable: false,
        enabled: always,
        run: |s, _| {
            Ok(json!({
                "width": s.doc.width(),
                "height": s.doc.height(),
                "objects": s.doc.objects,
                "selection": s.selected(),
                "meta": s.doc.meta,
            }))
        },
    },
];

/// The command with this id.
pub fn find(id: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|c| c.id == id)
}

// ---- parameters ----

fn num(p: &Value, key: &str) -> Option<f32> {
    p.get(key).and_then(Value::as_f64).map(|v| v as f32).filter(|v| v.is_finite())
}

fn need(p: &Value, key: &str) -> Result<f32> {
    num(p, key).ok_or_else(|| EngineError::Params(format!("`{key}` must be a number")))
}

fn int(p: &Value, key: &str, default: i64, lo: i64, hi: i64) -> Result<i64> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => {
            let n = v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| EngineError::Params(format!("`{key}` must be a number")))?;
            Ok((n.round() as i64).clamp(lo, hi))
        }
    }
}

fn boolean(p: &Value, key: &str, default: bool) -> bool {
    p.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn color(p: &Value, key: &str, default: Color) -> Result<Color> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::String(s)) => Color::from_hex(s).ok_or_else(|| EngineError::Params(format!("`{key}` must be a colour like #RRGGBB"))),
        Some(_) => Err(EngineError::Params(format!("`{key}` must be a colour like #RRGGBB"))),
    }
}

/// The ids a command acts on: `ids` if given, else the selection (unless `required`).
fn ids_param(s: &Session, p: &Value, default_selection: bool) -> Result<Vec<ObjectId>> {
    match p.get("ids") {
        None | Some(Value::Null) if default_selection => Ok(s.selected()),
        None | Some(Value::Null) => Err(EngineError::Params("`ids` is a list of annotation ids".to_string())),
        Some(Value::Array(a)) => {
            let ids: Vec<ObjectId> = a.iter().filter_map(Value::as_u64).map(ObjectId).collect();
            if ids.len() != a.len() {
                return Err(EngineError::Params("`ids` is a list of annotation ids".to_string()));
            }
            for id in &ids {
                if s.doc.get(*id).is_none() {
                    return Err(EngineError::NoObject(id.0));
                }
            }
            Ok(ids)
        }
        Some(_) => Err(EngineError::Params("`ids` is a list of annotation ids".to_string())),
    }
}

fn id_param(s: &Session, p: &Value) -> Result<ObjectId> {
    let id = p.get("id").and_then(Value::as_u64).map(ObjectId).ok_or_else(|| EngineError::Params("`id` is an annotation id".to_string()))?;
    if s.doc.get(id).is_none() {
        return Err(EngineError::NoObject(id.0));
    }
    Ok(id)
}

/// A style from JSON, merged over `base` field by field.
pub fn merge_style(base: &Style, patch: &Value) -> Result<Style> {
    let Value::Object(patch) = patch else { return Err(EngineError::Params("`style` must be an object".to_string())) };
    let mut v = serde_json::to_value(base).map_err(|e| EngineError::Params(e.to_string()))?;
    if let Value::Object(map) = &mut v {
        for (k, val) in patch {
            map.insert(k.clone(), val.clone());
        }
    }
    let style: Style = serde_json::from_value(v).map_err(|e| EngineError::Params(format!("style: {e}")))?;
    Ok(style.clamped())
}

// ---- edit ----

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = ids_param(s, p, true)?;
    let n = s.doc.remove(&ids);
    s.selection.retain(|id| !ids.contains(id));
    Ok(json!({ "deleted": n }))
}

fn duplicate(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = ids_param(s, p, true)?;
    let d = Pt::new(num(p, "dx").unwrap_or(16.0), num(p, "dy").unwrap_or(16.0));
    let mut new_ids = Vec::new();
    for id in ids {
        if let Some(o) = s.doc.get(id).cloned() {
            let mut shape = o.shape.clone();
            shape.translate(d);
            if let Shape::Step { number, .. } = &mut shape {
                *number = s.doc.next_step_number();
            }
            new_ids.push(s.doc.add(shape, o.style)?);
        }
    }
    s.selection = new_ids.clone();
    Ok(json!({ "ids": new_ids }))
}

fn nudge(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = ids_param(s, p, true)?;
    let d = Pt::new(num(p, "dx").unwrap_or(0.0), num(p, "dy").unwrap_or(0.0));
    for id in ids {
        if let Some(o) = s.doc.get_mut(id) {
            o.shape.translate(d);
        }
    }
    Ok(Value::Null)
}

// ---- annotations ----

/// An object from JSON (`{"type": …, …, "style"?: …}`), its style defaulting to its kind's.
pub fn parse_object(v: &Value) -> Result<(Shape, Style)> {
    let Value::Object(map) = v else { return Err(EngineError::Params("`object` must be an object with a `type`".to_string())) };
    let mut fields = map.clone();
    let style_patch = fields.remove("style");
    fields.remove("id");
    // The step number is filled in by the caller when missing.
    if fields.get("type").and_then(Value::as_str) == Some("step") && !fields.contains_key("number") {
        fields.insert("number".to_string(), json!(0));
    }
    let shape: Shape = serde_json::from_value(Value::Object(fields)).map_err(|e| EngineError::Params(format!("object: {e}")))?;
    let kind = shape.kind();
    let style = match style_patch {
        Some(p) => merge_style(&kind.default_style(), &p)?,
        None => kind.default_style(),
    };
    Ok((shape, style))
}

fn add_object(s: &mut Session, p: &Value) -> Result<Value> {
    let v = p.get("object").ok_or_else(|| EngineError::Params("`object` is required".to_string()))?;
    let (mut shape, style) = parse_object(v)?;
    if let Shape::Step { number, .. } = &mut shape
        && *number == 0
    {
        *number = s.doc.next_step_number();
    }
    let id = s.doc.add(shape, style)?;
    s.selection = vec![id];
    Ok(json!({ "id": id }))
}

fn update_object(s: &mut Session, p: &Value) -> Result<Value> {
    let id = id_param(s, p)?;
    let Some(current) = s.doc.get(id).cloned() else { return Err(EngineError::NoObject(id.0)) };
    let mut shape = current.shape.clone();
    if let Some(Value::Object(patch)) = p.get("object") {
        let mut v = serde_json::to_value(&current.shape).map_err(|e| EngineError::Params(e.to_string()))?;
        if let Value::Object(map) = &mut v {
            for (k, val) in patch {
                if k != "type" {
                    map.insert(k.clone(), val.clone());
                }
            }
        }
        shape = serde_json::from_value::<Shape>(v).map_err(|e| EngineError::Params(format!("object: {e}")))?.sanitized();
    }
    let style = match p.get("style") {
        Some(patch) => merge_style(&current.style, patch)?,
        None => current.style,
    };
    if let Some(o) = s.doc.get_mut(id) {
        o.shape = shape;
        o.style = style;
    }
    Ok(Value::Null)
}

fn restyle(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = ids_param(s, p, true)?;
    let patch = p.get("style").ok_or_else(|| EngineError::Params("`style` is required".to_string()))?;
    for id in ids {
        if let Some(o) = s.doc.get_mut(id) {
            o.style = merge_style(&o.style, patch)?;
        }
    }
    Ok(Value::Null)
}

fn set_text(s: &mut Session, p: &Value) -> Result<Value> {
    let id = id_param(s, p)?;
    let text = p.get("text").and_then(Value::as_str).ok_or_else(|| EngineError::Params("`text` is required".to_string()))?;
    let o = s.doc.get_mut(id).ok_or(EngineError::NoObject(id.0))?;
    match o.shape.text_mut() {
        Some(t) => {
            *t = text.chars().take(screengrabcraft_model::object::MAX_TEXT / 4).collect();
            Ok(Value::Null)
        }
        None => Err(EngineError::Params(format!("annotation {} has no text", id.0))),
    }
}

fn order(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = ids_param(s, p, true)?;
    let to: Order = serde_json::from_value(p.get("to").cloned().unwrap_or(Value::Null))
        .map_err(|_| EngineError::Params("`to` is \"front\", \"back\", \"forward\" or \"backward\"".to_string()))?;
    s.doc.reorder(&ids, to);
    Ok(Value::Null)
}

fn flatten(s: &mut Session, _: &Value) -> Result<Value> {
    let pic = screengrabcraft_render::render(&s.doc).ok_or_else(|| EngineError::Failed("the picture could not be drawn".to_string()))?;
    s.doc.base = pic;
    s.doc.objects.clear();
    s.doc.images.clear();
    s.selection.clear();
    Ok(Value::Null)
}

// ---- the whole picture ----

/// Moves every annotation by the change of the picture's origin.
fn shift_all(doc: &mut Document, dx: f32, dy: f32) {
    doc.map_objects(&|q| Pt::new(q.x + dx, q.y + dy), 1.0);
}

fn crop(s: &mut Session, p: &Value) -> Result<Value> {
    let area = Rect::new(need(p, "x")?, need(p, "y")?, need(p, "w")?, need(p, "h")?).finite();
    let (x, y, _, _) =
        area.pixel_bounds(s.doc.width(), s.doc.height()).ok_or_else(|| EngineError::Params("the area is outside the picture".to_string()))?;
    s.doc.base = effects::crop(&s.doc.base, area)?;
    shift_all(&mut s.doc, -(x as f32), -(y as f32));
    Ok(json!({ "width": s.doc.width(), "height": s.doc.height() }))
}

fn resize(s: &mut Session, p: &Value) -> Result<Value> {
    let (w0, h0) = (s.doc.width() as f32, s.doc.height() as f32);
    let (w, h) = if let Some(pc) = num(p, "percent") {
        let f = pc / 100.0;
        ((w0 * f).round(), (h0 * f).round())
    } else {
        match (num(p, "width"), num(p, "height")) {
            (Some(w), Some(h)) => (w.round(), h.round()),
            (Some(w), None) => (w.round(), (h0 * w / w0).round()),
            (None, Some(h)) => ((w0 * h / h0).round(), h.round()),
            (None, None) => return Err(EngineError::Params("give `width`, `height` or `percent`".to_string())),
        }
    };
    if !(1.0..=f32::from(u16::MAX)).contains(&w) || !(1.0..=f32::from(u16::MAX)).contains(&h) {
        return Err(EngineError::Params("the new size must be 1 to 65535 pixels a side".to_string()));
    }
    s.doc.base = effects::resize(&s.doc.base, w as u32, h as u32)?;
    let (sx, sy) = (w / w0, h / h0);
    s.doc.map_objects(&|q| Pt::new(q.x * sx, q.y * sy), (sx * sy).sqrt());
    Ok(json!({ "width": s.doc.width(), "height": s.doc.height() }))
}

fn canvas(s: &mut Session, p: &Value) -> Result<Value> {
    let lim = i64::from(screengrabcraft_model::MAX_SIDE);
    let (l, t) = (int(p, "left", 0, -lim, lim)? as i32, int(p, "top", 0, -lim, lim)? as i32);
    let (r, b) = (int(p, "right", 0, -lim, lim)? as i32, int(p, "bottom", 0, -lim, lim)? as i32);
    let fill = color(p, "color", Color::TRANSPARENT)?;
    s.doc.base = effects::canvas_size(&s.doc.base, l, t, r, b, fill)?;
    shift_all(&mut s.doc, l as f32, t as f32);
    Ok(json!({ "width": s.doc.width(), "height": s.doc.height() }))
}

fn rotate(s: &mut Session, p: &Value) -> Result<Value> {
    let deg = int(p, "degrees", 90, -36_000, 36_000)?;
    if deg % 90 != 0 {
        return Err(EngineError::Params("`degrees` is a multiple of 90".to_string()));
    }
    let quarters = (deg / 90).rem_euclid(4) as u32;
    let (w, h) = (s.doc.width() as f32, s.doc.height() as f32);
    s.doc.base = effects::rotate(&s.doc.base, quarters)?;
    let f: Box<dyn Fn(Pt) -> Pt> = match quarters {
        1 => Box::new(move |q: Pt| Pt::new(h - q.y, q.x)),
        2 => Box::new(move |q: Pt| Pt::new(w - q.x, h - q.y)),
        3 => Box::new(move |q: Pt| Pt::new(q.y, w - q.x)),
        _ => Box::new(|q: Pt| q),
    };
    s.doc.map_objects(&*f, 1.0);
    Ok(Value::Null)
}

fn flip(s: &mut Session, p: &Value) -> Result<Value> {
    let horizontal = boolean(p, "horizontal", true);
    let (w, h) = (s.doc.width() as f32, s.doc.height() as f32);
    s.doc.base = effects::flip(&s.doc.base, horizontal)?;
    if horizontal {
        s.doc.map_objects(&|q| Pt::new(w - q.x, q.y), 1.0);
    } else {
        s.doc.map_objects(&|q| Pt::new(q.x, h - q.y), 1.0);
    }
    Ok(Value::Null)
}

fn trim(s: &mut Session, p: &Value) -> Result<Value> {
    let tol = int(p, "tolerance", 8, 0, 255)? as u8;
    let area = effects::trim_bounds(&s.doc.base, tol);
    crop(s, &json!({ "x": area.x, "y": area.y, "w": area.w, "h": area.h }))
}

fn border(s: &mut Session, p: &Value) -> Result<Value> {
    let w = int(p, "width", 4, 1, 500)? as u32;
    let c = color(p, "color", Color::BLACK)?;
    s.doc.base = effects::border(&s.doc.base, w, c)?;
    shift_all(&mut s.doc, w as f32, w as f32);
    Ok(Value::Null)
}

fn shadow(s: &mut Session, p: &Value) -> Result<Value> {
    let off = int(p, "offset", 8, 0, 200)? as u32;
    let soft = int(p, "softness", 10, 1, 100)? as u32;
    let c = color(p, "color", Color::rgba(0, 0, 0, 128))?;
    let (l, t, _, _) = effects::shadow_margins(off, soft);
    s.doc.base = effects::drop_shadow(&s.doc.base, off, soft, c)?;
    shift_all(&mut s.doc, l as f32, t as f32);
    Ok(Value::Null)
}

fn torn(s: &mut Session, p: &Value) -> Result<Value> {
    let edges = effects::Edges {
        top: boolean(p, "top", false),
        right: boolean(p, "right", false),
        bottom: boolean(p, "bottom", true),
        left: boolean(p, "left", false),
    };
    let depth = int(p, "depth", 14, 4, 200)? as u32;
    s.doc.base = effects::torn_edge(&s.doc.base, edges, depth)?;
    Ok(Value::Null)
}

fn round(s: &mut Session, p: &Value) -> Result<Value> {
    let r = int(p, "radius", 16, 1, 1000)? as f32;
    s.doc.base = effects::round_corners(&s.doc.base, r)?;
    Ok(Value::Null)
}

/// Every command as JSON (for `screengrabcraft-cli commands` and agents).
pub fn describe() -> Value {
    Value::Array(
        COMMANDS
            .iter()
            .map(|c| json!({ "id": c.id, "label": c.label, "group": c.group, "shortcut": c.shortcut, "params": c.params, "undoable": c.undoable }))
            .collect(),
    )
}

/// Annotation objects as JSON values (used by `doc.info` and the CLI).
pub fn objects_json(objects: &[Object]) -> Value {
    serde_json::to_value(objects).unwrap_or(Value::Null)
}
