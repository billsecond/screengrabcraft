//! Every editing action as a command: an id, a label, where it belongs, its shortcut, what its
//! parameters are, and the function that does it. The editor's menus, toolbar and keyboard, the
//! command-line program and scripts all run commands by id, so they all behave the same.
//!
//! Parameters are JSON and can come from anywhere (a script, the command line): every one is
//! checked, and a command given nonsense returns an error and changes nothing.

use screengrabcraft_model::{Color, Document, Kind, Object, ObjectId, Order, Pt, Rect, Shape, ShapeOptions, Style};
use screengrabcraft_render::matte::{self, Region};
use screengrabcraft_render::{effects, region};
use serde_json::{Value, json};

use crate::arrange;
use crate::compose;
use crate::fx;
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
        r##"{"object": {"type": "arrow"|"line"|"rect"|"ellipse"|"polygon"|"pen"|"highlight"|"text"|"callout"|"step"|"blur"|"pixelate"|"spotlight"|"magnify"|"stamp", …its fields, "style"?: {…}}} → {"id": id}. Shapes are in picture pixels: arrow/line {"from":{"x","y"},"to":{"x","y"}} (arrow also "bend"?: 0.3, how far its middle bows to the left of the way it points as a share of its length, -1.5 to 1.5, and "head"?: "notched"|"triangle"|"open"|"dot"|"bar"), boxes {"rect":{"x","y","w","h"}}, polygon {"rect", "sides"?: 3 (3 to 12), "star"?: false}, pen {"points":[…]} (highlight is a box: points are made the box round them), text/callout {"text": "…"} (callout also "tail"), step {"center", "number"?, "labels"?: "numbers"|"upper"|"lower"|"roman"|"romanLower", "badge"?: "circle"|"rounded"|"square"|"diamond"|"hexagon"|"pin"} (a step without a number counts on from the others written the same way), stamp {"stamp": "check"|"cross"|"star"|"question"|"warning"|"info"|"heart"|"plus"|"minus"|"arrowUp"|"arrowDown"|"arrowLeft"|"arrowRight"|"noEntry"|"idea"|"pin"|"flag"|"lock"|"clock"|"search"|"bookmark"|"gear"|"home"|"mail"|"pencil"|"happy"|"laughing"|"wink"|"neutral"|"sad"|"surprised"|"angry"|"love"|"cool"}"##,
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
        r##"{"ids"?: [id], "style": {"stroke"?: "#RRGGBB", "fill"?, "width"?, "opacity"?, "shadow"?, "fontSize"?, "bold"?, "font"?: "a font on the computer by name (empty for the built-in one)", …}}"##,
        has_selection,
        restyle
    ),
    cmd!(
        "object.look",
        "Apply Quick Style",
        "Annotate",
        None,
        r##"{"ids"?: [id], "style"?: {…the look's style}, "options"?: {"heads"?: "end"|"start"|"both"|"none", "radius"?: 8, "form"?: "rounded"|"square"|"oval"|"cloud"|"arrow", "tail"?: "downLeft"|"downRight"|"upLeft"|"upRight"|"left"|"right"|"up"|"down", "stamp"?: "check"|…, "round"?: true, "bend"?: 0.3, "head"?: "notched"|"triangle"|"open"|"dot"|"bar", "sides"?: 5, "star"?: true, "labels"?: "numbers"|"upper"|"lower"|"roman"|"romanLower", "badge"?: "circle"|"rounded"|"square"|"diamond"|"hexagon"|"pin"}, "keepTextSize"?: true} — a quick style on the selection (the text size stays unless keepTextSize is false)"##,
        has_selection,
        apply_look
    ),
    cmd!("object.text", "Change Text", "Annotate", None, r#"{"id": id, "text": "…"}"#, has_objects, set_text),
    cmd!("object.order", "Arrange", "Annotate", None, r#"{"to": "front"|"back"|"forward"|"backward", "ids"?: [id]}"#, has_selection, order),
    cmd!(
        "object.rotate",
        "Rotate",
        "Annotate",
        None,
        r#"{"degrees": 15, "ids"?: [id]} turns them by that much round the middle of them all (the way a clock goes), or {"angle": 0} turns each box to that angle round its own middle. Boxes keep their turn; lines, arrows and strokes have their points turned; blur, pixelate, spotlight and magnify stay square"#,
        has_selection,
        arrange::rotate
    ),
    cmd!(
        "object.group",
        "Group",
        "Annotate",
        Some("Ctrl+G"),
        r#"{"ids"?: [id]} (two or more) → {"group": n}: selected, moved, turned and lined up as one"#,
        has_selection,
        arrange::group
    ),
    cmd!("object.ungroup", "Ungroup", "Annotate", Some("Ctrl+Shift+G"), r#"{"ids"?: [id]} → {"ungrouped": n}"#, has_selection, arrange::ungroup),
    cmd!(
        "object.align",
        "Align",
        "Annotate",
        None,
        r#"{"to": "left"|"center"|"right"|"top"|"middle"|"bottom", "ids"?: [id], "picture"?: false} — on the box round them all (a group as one); one alone, or with "picture", on the picture"#,
        has_selection,
        arrange::align
    ),
    cmd!(
        "object.distribute",
        "Distribute",
        "Annotate",
        None,
        r#"{"horizontal"?: true, "ids"?: [id]} — three or more (a group as one) spread out evenly across (or down), the first and the last staying"#,
        has_selection,
        arrange::distribute
    ),
    cmd!(
        "object.flip",
        "Flip Annotations",
        "Annotate",
        None,
        r#"{"horizontal"?: true, "ids"?: [id]} — mirrored round the middle of them all; words and pictures still read the right way round"#,
        has_selection,
        arrange::flip
    ),
    cmd!(
        "edit.selectKind",
        "Select All of a Kind",
        "Edit",
        None,
        r#"{"kind"?: "arrow"|"line"|"rect"|"ellipse"|"polygon"|"pen"|"highlight"|"text"|"callout"|"step"|"blur"|"pixelate"|"spotlight"|"magnify"|"stamp"|"image"} (default: the kinds selected) → {"selected": n}"#,
        has_objects,
        arrange::select_kind
    ),
    cmd!(
        "object.flatten",
        "Flatten",
        "Annotate",
        Some("Ctrl+Shift+F"),
        r#"{"ids"?: [id]} — draws those annotations (default: every one) into the picture for good"#,
        has_objects,
        flatten
    ),
    cmd!(
        "image.lift",
        "Move Area",
        "Image",
        None,
        r#"{"x": 0, "y": 0, "w": 100, "h": 100, "shape"?: "rect"|"ellipse"|"freehand"|"wand", "points"?: [{"x", "y"}] (freehand), "seed"?: {"x", "y"}, "tolerance"?: 32, "contiguous"?: true (wand), "invert"?: false} — takes that part out of the picture as a placed picture, selected, leaving it see-through; returns {"id": id}"#,
        always,
        lift
    ),
    cmd!(
        "image.erase",
        "Delete Area",
        "Image",
        None,
        r#"{"x": 0, "y": 0, "w": 100, "h": 100, "shape"?: …, "invert"?: false} (as image.lift) — makes that part of the picture see-through"#,
        always,
        erase
    ),
    cmd!(
        "image.crop",
        "Crop",
        "Image",
        None,
        r#"{"x": 0, "y": 0, "w": 100, "h": 100, "shape"?: …, "invert"?: false} (as image.lift: a shape is cropped to the box round it, see-through outside it)"#,
        always,
        crop
    ),
    cmd!(
        "image.eraseStroke",
        "Erase",
        "Image",
        None,
        r##"{"points": [{"x", "y"}], "width"?: 20, "color"?: "#RRGGBB"} — the eraser: what the stroke passes over becomes see-through (or the colour)"##,
        always,
        erase_stroke
    ),
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
    cmd!(
        "image.combine",
        "Combine Pictures",
        "Image",
        None,
        r##"{"layout"?: "across"|"down"|"grid", "columns"?: 2 (grid), "gap"?: 16 (0 to 400 px), "background"?: "#FFFFFF"} — this picture and the ones the host lent (the command line's --picture) side by side, stacked or in a grid, on one picture; the annotations stay on this one"##,
        always,
        compose::combine
    ),
    cmd!(
        "image.template",
        "Template",
        "Image",
        None,
        r##"{"layout"?: "stepsAcross"|"stepsDown"|"beforeAfter"|"grid", "title"?: "…", "steps"?: ["the words of each step"], "background"?: "#F4F6FA", "accent"?: "#1E88E5", "size"?: 480} — this picture (as shown) and the ones lent, laid out on a page with a title, step numbers and their words, all annotations to change"##,
        always,
        compose::template
    ),
    cmd!(
        "image.cutOut",
        "Cut Out",
        "Image",
        None,
        r#"{"from": px, "to": px, "rows"?: false, "edge"?: "straight"|"saw"|"wave"} — takes the columns (or rows) from..to out of the picture and joins the rest"#,
        always,
        cut_out
    ),
    cmd!(
        "image.fill",
        "Fill",
        "Image",
        None,
        r##"{"x": px, "y": px, "color": "#RRGGBB"|"#RRGGBBAA", "tolerance"?: 32} → {"filled": pixels} — the area round the pixel at x, y that is within tolerance (0 to 255 a channel) of its colour becomes the colour (a transparent one clears it)"##,
        always,
        fill_area
    ),
    cmd!(
        "image.removeBackground",
        "Remove Background",
        "Image",
        None,
        r#"{"model"?: "isnet-general-use-int8"} — a model tells the subject from the background, which becomes transparent with soft edges (the desktop editor and the command-line program have the model)"#,
        always,
        remove_background
    ),
    cmd!(
        "image.removeSolidBackground",
        "Remove Solid Background",
        "Image",
        None,
        r##"{"tolerance"?: 24, "color"?: "#RRGGBB", "shadows"?: true} — the plain colour round the picture's edges (theirs unless given) becomes transparent up to the subject; its soft shadows become see-through black"##,
        always,
        remove_solid_background
    ),
    cmd!(
        "image.makeColorTransparent",
        "Make a Colour Transparent",
        "Image",
        None,
        r##"{"color"?: "#RRGGBB", "x"?: px, "y"?: px, "tolerance"?: 24, "contiguous"?: false, "shadows"?: true} — a colour (given, or the pixel at x, y) becomes transparent everywhere, or only where it joins x, y"##,
        always,
        make_color_transparent
    ),
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
    cmd!(
        "effect.wavyEdge",
        "Wavy Edge",
        "Effects",
        None,
        r#"{"top"?: false, "right"?: false, "bottom"?: true, "left"?: false, "depth"?: 10 (2 to 100), "length"?: 40 (8 to 400, from crest to crest)}"#,
        always,
        fx::wavy
    ),
    cmd!(
        "effect.fadedEdge",
        "Faded Edge",
        "Effects",
        None,
        r#"{"top"?: true, "right"?: true, "bottom"?: true, "left"?: true, "width"?: 30 (2 to 400)} — the picture fades away to see-through at those sides"#,
        always,
        fx::faded
    ),
    cmd!(
        "effect.bevel",
        "Bevel",
        "Effects",
        None,
        r#"{"width"?: 8 (1 to 100), "strength"?: 50 (0 to 100)} — a raised edge, lit from the top left"#,
        always,
        fx::bevel
    ),
    cmd!(
        "effect.perspective",
        "Perspective",
        "Effects",
        None,
        r#"{"horizontal"?: 30, "vertical"?: 0} (-100 to 100) — tilts the picture away: the right side (the left below nought), the bottom (the top); the annotations move with it"#,
        always,
        fx::perspective
    ),
    cmd!(
        "effect.pageCurl",
        "Page Curl",
        "Effects",
        None,
        r#"{"corner"?: "bottomRight"|"bottomLeft"|"topRight"|"topLeft", "size"?: 20 (5 to 60, hundredths of the shorter side)}"#,
        always,
        fx::page_curl
    ),
    cmd!(
        "effect.reflection",
        "Reflection",
        "Effects",
        None,
        r#"{"height"?: 35 (5 to 100, hundredths of the picture's height), "opacity"?: 40 (0 to 100), "gap"?: 0 (0 to 200 px)} — the picture grows downwards to hold it"#,
        always,
        fx::reflection
    ),
    cmd!("effect.grayscale", "Grayscale", "Effects", None, r#"{"area"?: {"x", "y", "w", "h"}} (default: all of the picture)"#, always, fx::grayscale),
    cmd!("effect.invert", "Invert Colors", "Effects", None, r#"{"area"?: {"x", "y", "w", "h"}}"#, always, fx::invert),
    cmd!("effect.sepia", "Sepia", "Effects", None, r#"{"amount"?: 100 (0 to 100), "area"?: {"x", "y", "w", "h"}}"#, always, fx::sepia),
    cmd!(
        "effect.brightness",
        "Brightness and Contrast",
        "Effects",
        None,
        r#"{"brightness"?: 0, "contrast"?: 0 (-100 to 100), "area"?: {"x", "y", "w", "h"}}"#,
        always,
        fx::brightness
    ),
    cmd!(
        "effect.hueSaturation",
        "Hue, Saturation and Gamma",
        "Effects",
        None,
        r#"{"hue"?: 0 (-180 to 180 degrees round the colour wheel), "saturation"?: 0, "lightness"?: 0 (-100 to 100), "gamma"?: 1 (0.2 to 5; above 1 lightens the mid-tones), "area"?: {"x", "y", "w", "h"}}"#,
        always,
        fx::hue_saturation
    ),
    cmd!(
        "effect.sharpen",
        "Sharpen",
        "Effects",
        None,
        r#"{"amount"?: 60 (0 to 300), "radius"?: 1.5 (0.5 to 10 px), "area"?: {"x", "y", "w", "h"}}"#,
        always,
        fx::sharpen
    ),
    cmd!("effect.blur", "Blur", "Effects", None, r#"{"radius"?: 4 (0.5 to 100 px), "area"?: {"x", "y", "w", "h"}}"#, always, fx::blur),
    cmd!(
        "effect.watermark",
        "Watermark",
        "Effects",
        None,
        r##"{"text"?: "…" or "picture"?: 0 (a picture the host lent: the editor's chosen one, the command line's --picture), "place"?: "center"|"tiled"|"topLeft"|"top"|"topRight"|"left"|"right"|"bottomLeft"|"bottom"|"bottomRight", "size"?: 30 (2 to 100, hundredths of the picture's width), "angle"?: 0 (-90 to 90), "opacity"?: 30 (0 to 100), "under"?: false (under: on the plain parts only, never over words and lines), "color"?: "#FFFFFF", "bold"?: true, "font"?: "a font on the computer"}"##,
        always,
        fx::watermark
    ),
    cmd!(
        "effect.captureDetails",
        "Capture Details",
        "Effects",
        None,
        r##"{"time"?: true|false|"…", "program"?: true|false|"…", "size"?: true, "place"?: "bottomRight"|"bottomLeft"|"topRight"|"topLeft"|"below", "fontSize"?: 14, "color"?: "#FFFFFF", "background"?: "#000000A0"} — writes when it was captured, what of, and its size on it (true: as the document knows them; text: written as given); "below" adds a strip under the picture → {"text"}"##,
        always,
        fx::capture_details
    ),
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

pub(crate) fn num(p: &Value, key: &str) -> Option<f32> {
    p.get(key).and_then(Value::as_f64).map(|v| v as f32).filter(|v| v.is_finite())
}

fn need(p: &Value, key: &str) -> Result<f32> {
    num(p, key).ok_or_else(|| EngineError::Params(format!("`{key}` must be a number")))
}

pub(crate) fn int(p: &Value, key: &str, default: i64, lo: i64, hi: i64) -> Result<i64> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => {
            let n = v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| EngineError::Params(format!("`{key}` must be a number")))?;
            Ok((n.round() as i64).clamp(lo, hi))
        }
    }
}

pub(crate) fn boolean(p: &Value, key: &str, default: bool) -> bool {
    p.get(key).and_then(Value::as_bool).unwrap_or(default)
}

pub(crate) fn color(p: &Value, key: &str, default: Color) -> Result<Color> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::String(s)) => Color::from_hex(s).ok_or_else(|| EngineError::Params(format!("`{key}` must be a colour like #RRGGBB"))),
        Some(_) => Err(EngineError::Params(format!("`{key}` must be a colour like #RRGGBB"))),
    }
}

/// The ids a command acts on: `ids` if given, else the selection (unless `required`).
pub(crate) fn ids_param(s: &Session, p: &Value, default_selection: bool) -> Result<Vec<ObjectId>> {
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
    // Against an edge the copies go the other way, not out of sight on top of the originals.
    let (on, back) = (s.doc.limit_move(&ids, d), s.doc.limit_move(&ids, d.scale(-1.0)));
    let further = |a: f32, b: f32| if a.abs() >= b.abs() { a } else { b };
    let d = Pt::new(further(on.x, back.x), further(on.y, back.y));
    let mut new_ids = Vec::new();
    // Copies of a group are a group of their own.
    let mut groups: Vec<(u64, u64)> = Vec::new();
    for id in ids {
        if let Some(o) = s.doc.get(id).cloned() {
            let mut shape = o.shape.clone();
            shape.translate(d);
            if let Shape::Step { number, labels, .. } = &mut shape {
                *number = s.doc.next_step_number(*labels);
            }
            let copy = s.doc.add(shape, o.style)?;
            let group = o.group.map(|g| match groups.iter().find(|(old, _)| *old == g) {
                Some((_, new)) => *new,
                None => {
                    let new = s.doc.next_group();
                    groups.push((g, new));
                    new
                }
            });
            if let Some(c) = s.doc.get_mut(copy) {
                c.rotation = o.rotation;
                c.group = group;
            }
            new_ids.push(copy);
        }
    }
    s.selection = new_ids.clone();
    Ok(json!({ "ids": new_ids }))
}

fn nudge(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = ids_param(s, p, true)?;
    // Past an edge the picture grows to hold them, see-through where it is new.
    let d = s.doc.limit_move_near(&ids, Pt::new(num(p, "dx").unwrap_or(0.0), num(p, "dy").unwrap_or(0.0)), screengrabcraft_model::document::MAX_GROW);
    for id in &ids {
        if let Some(o) = s.doc.get_mut(*id) {
            o.shape.translate(d);
        }
    }
    if let Some(area) = s.doc.bounds_of(&ids) {
        s.doc.grow_to(area);
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
    if let Shape::Step { number, labels, .. } = &mut shape
        && *number == 0
    {
        *number = s.doc.next_step_number(*labels);
    }
    let id = s.doc.add(shape, style)?;
    s.selection = vec![id];
    Ok(json!({ "id": id }))
}

fn update_object(s: &mut Session, p: &Value) -> Result<Value> {
    let id = id_param(s, p)?;
    let Some(current) = s.doc.get(id).cloned() else { return Err(EngineError::NoObject(id.0)) };
    let style = match p.get("style") {
        Some(patch) => merge_style(&current.style, patch)?,
        None => current.style,
    };
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
        shape = serde_json::from_value::<Shape>(v).map_err(|e| EngineError::Params(format!("object: {e}")))?.sanitized(style.width);
    }
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

fn apply_look(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = ids_param(s, p, true)?;
    let options = match p.get("options") {
        None | Some(Value::Null) => ShapeOptions::default(),
        Some(v) => serde_json::from_value::<ShapeOptions>(v.clone()).map_err(|e| EngineError::Params(format!("options: {e}")))?.clamped(),
    };
    let style_patch = match p.get("style") {
        None | Some(Value::Null) => None,
        Some(v @ Value::Object(_)) => Some(v),
        Some(_) => return Err(EngineError::Params("`style` must be an object".to_string())),
    };
    let keep_size = match p.get("keepTextSize") {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(EngineError::Params("`keepTextSize` is true or false".to_string())),
    };
    // Everything worked out first, so nonsense changes nothing.
    let picture = s.doc.bounds();
    let mut changes = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(o) = s.doc.get(id) else { return Err(EngineError::NoObject(id.0)) };
        let mut style = match style_patch {
            Some(patch) => merge_style(&o.style, patch)?,
            None => o.style,
        };
        if keep_size {
            style.font_size = o.style.font_size;
        }
        let mut shape = options.applied(o.shape.clone());
        // A tail set to a spot stays on the picture.
        if let Shape::Callout { tail, .. } = &mut shape {
            *tail = picture.nearest(*tail);
        }
        // Bigger text (a title) gets the room it needs below; a box is never made smaller.
        if style.font_size > o.style.font_size {
            match &mut shape {
                Shape::Text { rect, text } => {
                    rect.h = rect.h.max(screengrabcraft_render::text::fit_box(text, &style, rect.min(), Some(rect.w)).h);
                }
                Shape::Callout { rect, text, form, .. } => {
                    let pad = screengrabcraft_render::text::padding(&style);
                    let lay = screengrabcraft_render::text::layout(text, &style, Some(form.text_area(*rect).w - 2.0 * pad));
                    rect.h = rect.h.max(form.box_height(lay.height + 2.0 * pad));
                }
                _ => {}
            }
        }
        changes.push((id, picture.encloses(o.shape.bounds(&o.style)), shape, style));
    }
    for (id, was_inside, shape, style) in changes {
        if let Some(o) = s.doc.get_mut(id) {
            o.shape = shape;
            o.style = style;
        }
        // A thicker line or a longer tail doesn't push it over the edge.
        if was_inside {
            s.doc.bring_inside(id);
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

fn flatten(s: &mut Session, p: &Value) -> Result<Value> {
    let failed = || EngineError::Failed("the picture could not be drawn".to_string());
    match p.get("ids") {
        None | Some(Value::Null) => {
            s.doc.base = screengrabcraft_render::render(&s.doc).ok_or_else(failed)?;
            s.doc.objects.clear();
            s.doc.images.clear();
            s.selection.clear();
        }
        Some(_) => {
            // Only those: the picture is drawn with them alone on it, and they go.
            let ids = ids_param(s, p, false)?;
            let hide: Vec<ObjectId> = s.doc.objects.iter().map(|o| o.id).filter(|id| !ids.contains(id)).collect();
            let pix = screengrabcraft_render::Renderer::new()
                .render(&s.doc, &screengrabcraft_render::Options { hide, no_text: None })
                .ok_or_else(failed)?;
            s.doc.base = screengrabcraft_render::canvas::from_pixmap(&pix).ok_or_else(failed)?;
            s.doc.remove(&ids);
            s.selection.retain(|id| !ids.contains(id));
        }
    }
    Ok(Value::Null)
}

/// The part of the picture a command works on: the box `x`, `y`, `w`, `h`, or with `"shape"`
/// an ellipse in that box, a freehand outline (`"points"`) or the pixels of a colour (the magic
/// wand: `"seed"`, `"tolerance"`, `"contiguous"`); with `"invert"` everything else.
pub(crate) fn region_param(s: &Session, p: &Value) -> Result<region::Region> {
    let shape = match p.get("shape") {
        None | Some(Value::Null) => region::AreaShape::Rect,
        Some(_) => serde_json::from_value::<region::AreaShape>(p.clone())
            .map_err(|e| {
                EngineError::Params(format!("`shape` is \"rect\", \"ellipse\", \"freehand\" (with \"points\") or \"wand\" (with \"seed\"): {e}"))
            })?
            .sanitized(),
    };
    let area = match shape {
        region::AreaShape::Rect | region::AreaShape::Ellipse => Rect::new(need(p, "x")?, need(p, "y")?, need(p, "w")?, need(p, "h")?).finite(),
        _ => Rect::default(),
    };
    region::region(&s.doc.base, area, &shape, boolean(p, "invert", false))
        .ok_or_else(|| EngineError::Params("the area is outside the picture, or nothing is in it".to_string()))
}

fn lift(s: &mut Session, p: &Value) -> Result<Value> {
    let reg = region_param(s, p)?;
    let piece = region::piece(&s.doc.base, &reg).ok_or_else(|| EngineError::Params("the area is outside the picture".to_string()))?;
    let key = s.doc.add_image(piece)?;
    s.doc.base = region::erase(&s.doc.base, &reg);
    // Where it was, looking as it did (no shadow, no border).
    let style = Style { shadow: false, ..Kind::Image.default_style() };
    let id = s.doc.add(Shape::Image { rect: reg.rect(), image: key }, style)?;
    s.selection = vec![id];
    Ok(json!({ "id": id.0 }))
}

fn erase(s: &mut Session, p: &Value) -> Result<Value> {
    let reg = region_param(s, p)?;
    s.doc.base = region::erase(&s.doc.base, &reg);
    Ok(Value::Null)
}

/// The eraser: what a stroke passes over made see-through, or a colour.
fn erase_stroke(s: &mut Session, p: &Value) -> Result<Value> {
    let points: Vec<Pt> = match p.get("points") {
        Some(Value::Array(a)) => {
            a.iter().take(region::MAX_OUTLINE).map(|v| serde_json::from_value::<Pt>(v.clone())).collect::<std::result::Result<_, _>>()
        }
        _ => return Err(EngineError::Params("`points` is a list of {\"x\", \"y\"}".to_string())),
    }
    .map_err(|_| EngineError::Params("`points` is a list of {\"x\", \"y\"}".to_string()))?;
    let width = num(p, "width").unwrap_or(20.0);
    let colour = color(p, "color", Color::TRANSPARENT)?;
    let Some(reg) = region::stroke(s.doc.width(), s.doc.height(), &points, width)? else {
        return Err(EngineError::Params("the stroke doesn't pass over the picture".to_string()));
    };
    s.doc.base = region::paint(&s.doc.base, &reg, colour);
    Ok(Value::Null)
}

// ---- the whole picture ----

/// Moves every annotation by the change of the picture's origin.
fn shift_all(doc: &mut Document, dx: f32, dy: f32) {
    doc.map_objects(&|q| Pt::new(q.x + dx, q.y + dy), 1.0);
}

/// Crop: to the box (or the box round a shape, see-through outside it).
fn crop(s: &mut Session, p: &Value) -> Result<Value> {
    let reg = region_param(s, p)?;
    s.doc.base = region::piece(&s.doc.base, &reg).ok_or_else(|| EngineError::Params("the area is outside the picture".to_string()))?;
    shift_all(&mut s.doc, -(reg.x as f32), -(reg.y as f32));
    Ok(json!({ "width": s.doc.width(), "height": s.doc.height() }))
}

/// Cut Out: the columns (or rows) `from..to` taken out and the rest joined; the annotations past
/// the strip move back with the picture, those in it go to the join.
fn cut_out(s: &mut Session, p: &Value) -> Result<Value> {
    use screengrabcraft_render::cut;
    let rows = boolean(p, "rows", false);
    let edge = match p.get("edge") {
        None | Some(Value::Null) => cut::Edge::default(),
        Some(Value::String(e)) => cut::Edge::from_name(e).ok_or_else(|| EngineError::Params("`edge` is straight, saw or wave".to_string()))?,
        Some(_) => return Err(EngineError::Params("`edge` is straight, saw or wave".to_string())),
    };
    let across = if rows { s.doc.height() } else { s.doc.width() } as f32;
    let (a, b) = (need(p, "from")?, need(p, "to")?);
    let (from, to) = (a.min(b).clamp(0.0, across).round(), a.max(b).clamp(0.0, across).round());
    if to - from < 1.0 {
        return Err(EngineError::Params("the strip to take out is empty".to_string()));
    }
    if to - from >= across {
        return Err(EngineError::Params("that would take out all of the picture".to_string()));
    }
    s.doc.base = cut::cut_out(&s.doc.base, from as u32, to as u32, !rows, edge)?;
    if rows {
        s.doc.map_objects(&|q| Pt::new(q.x, cut::moved(q.y, from, to)), 1.0);
    } else {
        s.doc.map_objects(&|q| Pt::new(cut::moved(q.x, from, to), q.y), 1.0);
    }
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

fn fill_area(s: &mut Session, p: &Value) -> Result<Value> {
    let (x, y) = pixel_param(s, p)?.ok_or_else(|| EngineError::Params("give the `x` and `y` of a pixel to fill from".to_string()))?;
    if p.get("color").is_none_or(Value::is_null) {
        return Err(EngineError::Params("`color` is the colour to fill with, like #RRGGBB".to_string()));
    }
    let colour = color(p, "color", Color::TRANSPARENT)?;
    let tolerance = int(p, "tolerance", 32, 0, 255)? as u8;
    let (picture, filled) = effects::fill(&s.doc.base, x, y, colour, tolerance)?;
    if filled > 0 {
        s.doc.base = picture;
    }
    Ok(json!({ "filled": filled }))
}

// ---- the background ----

/// A picture just taken away, or why nothing could be.
fn cleared(s: &mut Session, out: matte::Removed, colour: Color) -> Result<Value> {
    let all = u64::from(s.doc.width()) * u64::from(s.doc.height());
    if out.cleared == 0 && out.softened == 0 {
        return Err(EngineError::Failed(format!("nothing in the picture is close enough to {} to take away", colour.to_hex())));
    }
    if out.cleared >= all {
        return Err(EngineError::Failed(format!("the whole picture is {}, so nothing would be left", colour.to_hex())));
    }
    s.doc.base = out.picture;
    Ok(json!({ "color": colour.to_hex(), "cleared": out.cleared, "softened": out.softened }))
}

fn remove_solid_background(s: &mut Session, p: &Value) -> Result<Value> {
    let tolerance = int(p, "tolerance", 24, 0, 255)? as u8;
    let colour = match p.get("color") {
        None | Some(Value::Null) => matte::edge_colour(&s.doc.base).ok_or_else(|| {
            EngineError::Failed("the picture's edges aren't one plain colour, so there is no plain background to take away (Remove Background tells a subject from any background)".to_string())
        })?,
        Some(_) => color(p, "color", Color::TRANSPARENT)?,
    };
    if colour.a == 0 {
        return Err(EngineError::Failed("the background is transparent already".to_string()));
    }
    let out = matte::remove_colour(&s.doc.base, colour, tolerance, Region::Edges, boolean(p, "shadows", true))?;
    cleared(s, out, colour)
}

/// The pixel at `x`, `y`, if they are given.
fn pixel_param(s: &Session, p: &Value) -> Result<Option<(u32, u32)>> {
    let given = |k: &str| p.get(k).is_some_and(|v| !v.is_null());
    if !given("x") && !given("y") {
        return Ok(None);
    }
    let (x, y) = (need(p, "x")?.floor(), need(p, "y")?.floor());
    if x < 0.0 || y < 0.0 || x >= s.doc.width() as f32 || y >= s.doc.height() as f32 {
        return Err(EngineError::Params("the point is outside the picture".to_string()));
    }
    Ok(Some((x as u32, y as u32)))
}

fn make_color_transparent(s: &mut Session, p: &Value) -> Result<Value> {
    let tolerance = int(p, "tolerance", 24, 0, 255)? as u8;
    let at = pixel_param(s, p)?;
    let colour = match (p.get("color"), at) {
        (None | Some(Value::Null), Some((x, y))) => {
            s.doc.base.pixel(x, y).ok_or_else(|| EngineError::Params("the point is outside the picture".to_string()))?
        }
        (None | Some(Value::Null), None) => return Err(EngineError::Params("give a `color`, or the `x` and `y` of a pixel of it".to_string())),
        _ => color(p, "color", Color::TRANSPARENT)?,
    };
    let region = if boolean(p, "contiguous", false) {
        let (x, y) = at.ok_or_else(|| EngineError::Params("`contiguous` needs the `x` and `y` to start from".to_string()))?;
        Region::From(x, y)
    } else {
        Region::Everywhere
    };
    if colour.a == 0 {
        return Err(EngineError::Failed("that colour is transparent already".to_string()));
    }
    let out = matte::remove_colour(&s.doc.base, colour, tolerance, region, boolean(p, "shadows", true))?;
    cleared(s, out, colour)
}

fn remove_background(s: &mut Session, p: &Value) -> Result<Value> {
    let wanted = match p.get("model") {
        None | Some(Value::Null) => None,
        Some(Value::String(m)) => Some(m.clone()),
        Some(_) => return Err(EngineError::Params("`model` is the name of a model".to_string())),
    };
    let (name, found) = match s.background.take_prepared(&s.doc.base, wanted.as_deref()) {
        Some(ready) => ready,
        None => {
            let model = s
                .background
                .model()
                .cloned()
                .ok_or_else(|| EngineError::Failed("there is no background model here (Remove Solid Background needs none)".to_string()))?;
            if let Some(w) = &wanted
                && w != model.name()
            {
                return Err(EngineError::Params(format!("there is no model `{w}` here (there is `{}`)", model.name())));
            }
            let found = model.matte(&s.doc.base, &mut |_| true).map_err(|e| EngineError::Failed(format!("removing the background: {e}")))?;
            (model.name().to_string(), found)
        }
    };
    s.doc.base = matte::apply_clean(&s.doc.base, &found)?;
    Ok(json!({ "model": name }))
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
