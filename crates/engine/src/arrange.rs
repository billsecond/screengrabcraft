//! Arranging annotations: turning, grouping, lining up, spreading out, mirroring, and selecting
//! every one of a kind. Their entries are in [`crate::registry::COMMANDS`].

use screengrabcraft_model::{Align, Kind, ObjectId, Rect};
use serde_json::{Value, json};

use crate::registry::{boolean, ids_param, num};
use crate::session::Session;
use crate::{EngineError, Result};

/// After a change that may have taken them past an edge, the picture grows to hold them
/// (see-through where it is new), as a nudge does.
fn hold(s: &mut Session, ids: &[ObjectId]) {
    if let Some(area) = s.doc.bounds_of(ids) {
        s.doc.grow_to(area);
    }
}

pub(crate) fn rotate(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = s.doc.with_groups(&ids_param(s, p, true)?);
    match (num(p, "degrees"), num(p, "angle")) {
        (Some(by), None) => {
            // Round the middle of them all, as one.
            let pivot = s.doc.bounds_of(&ids).map(Rect::center).ok_or_else(|| EngineError::Params("nothing to turn".to_string()))?;
            s.doc.rotate_objects(&ids, by, pivot);
        }
        (None, Some(to)) => {
            // Each to that turn, round its own middle.
            for id in &ids {
                let Some(o) = s.doc.get(*id) else { continue };
                let (now, pivot) = (o.turn(), o.shape.pivot());
                if o.shape.turns() {
                    s.doc.rotate_objects(&[*id], to - now, pivot);
                }
            }
        }
        _ => return Err(EngineError::Params("give `degrees` to turn by, or `angle` to turn each to".to_string())),
    }
    hold(s, &ids);
    Ok(Value::Null)
}

pub(crate) fn group(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = s.doc.with_groups(&ids_param(s, p, true)?);
    if ids.len() < 2 {
        return Err(EngineError::Params("a group needs two annotations or more".to_string()));
    }
    let g = s.doc.next_group();
    for id in &ids {
        if let Some(o) = s.doc.get_mut(*id) {
            o.group = Some(g);
        }
    }
    s.selection = ids;
    Ok(json!({ "group": g }))
}

pub(crate) fn ungroup(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = s.doc.with_groups(&ids_param(s, p, true)?);
    let mut n = 0;
    for id in &ids {
        if let Some(o) = s.doc.get_mut(*id)
            && o.group.take().is_some()
        {
            n += 1;
        }
    }
    Ok(json!({ "ungrouped": n }))
}

pub(crate) fn align(s: &mut Session, p: &Value) -> Result<Value> {
    let to: Align = serde_json::from_value(p.get("to").cloned().unwrap_or(Value::Null))
        .map_err(|_| EngineError::Params("`to` is \"left\", \"center\", \"right\", \"top\", \"middle\" or \"bottom\"".to_string()))?;
    let ids = s.doc.with_groups(&ids_param(s, p, true)?);
    // One alone (or one group) lines up on the picture.
    let within = (s.doc.units(&ids).len() < 2 || boolean(p, "picture", false)).then(|| s.doc.bounds());
    s.doc.align(&ids, to, within);
    Ok(Value::Null)
}

pub(crate) fn distribute(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = s.doc.with_groups(&ids_param(s, p, true)?);
    if s.doc.units(&ids).len() < 3 {
        return Err(EngineError::Params("spreading out needs three annotations (or groups) or more".to_string()));
    }
    s.doc.distribute(&ids, boolean(p, "horizontal", true));
    Ok(Value::Null)
}

pub(crate) fn flip(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = s.doc.with_groups(&ids_param(s, p, true)?);
    s.doc.flip_objects(&ids, boolean(p, "horizontal", true));
    Ok(Value::Null)
}

pub(crate) fn select_kind(s: &mut Session, p: &Value) -> Result<Value> {
    let kinds: Vec<Kind> = match p.get("kind") {
        None | Some(Value::Null) => {
            let mut k: Vec<Kind> = s.selected().iter().filter_map(|id| s.doc.get(*id)).map(|o| o.shape.kind()).collect();
            k.dedup();
            if k.is_empty() {
                return Err(EngineError::Params("give a `kind`, or select one of that kind first".to_string()));
            }
            k
        }
        Some(v) => {
            vec![serde_json::from_value(v.clone()).map_err(|_| EngineError::Params("`kind` is a kind of annotation, like \"arrow\"".to_string()))?]
        }
    };
    let ids: Vec<ObjectId> = s.doc.objects.iter().filter(|o| kinds.contains(&o.shape.kind())).map(|o| o.id).collect();
    let n = ids.len();
    s.select(ids);
    Ok(json!({ "selected": n }))
}
