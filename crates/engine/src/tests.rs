use screengrabcraft_model::{Color, Document, Kind, ObjectId, Pt, Raster, Rect, Shape};
use serde_json::{Value, json};

use crate::{COMMANDS, EngineError, Session};

fn session() -> Session {
    Session::new(Document::new(Raster::filled(200, 100, Color::WHITE).unwrap()))
}

fn add(s: &mut Session, object: Value) -> ObjectId {
    let out = s.execute("object.add", &json!({ "object": object })).unwrap();
    ObjectId(out["id"].as_u64().unwrap())
}

#[test]
fn hostile_params_never_panic() {
    let junk = [
        json!(null),
        json!(1),
        json!("x"),
        json!([]),
        json!({}),
        json!({"ids": "all", "id": -1, "x": "NaN", "w": 1e300, "h": -5, "degrees": 45, "percent": 1e9}),
        json!({"ids": [1, 2, 99], "dx": 1e30, "dy": null, "style": {"width": "wide", "stroke": 7}}),
        json!({"object": {"type": "teapot"}}),
        json!({"object": {"type": "pen", "points": [{"x": 1e40, "y": -1e40}]}}),
        json!({"object": {"type": "text", "rect": {"x": 0, "y": 0, "w": -5, "h": 1e39}, "text": "é".repeat(50_000)}}),
        json!({"width": 0, "height": 70000, "left": -1000, "right": 1e12, "color": "#zzz", "radius": -3, "to": "sideways"}),
        json!({"id": 1, "object": {"type": "line", "from": 3}, "text": 5}),
    ];
    for c in COMMANDS {
        for p in &junk {
            let mut s = session();
            let a = add(&mut s, json!({"type": "rect", "rect": {"x": 10, "y": 10, "w": 50, "h": 30}}));
            add(&mut s, json!({"type": "text", "rect": {"x": 70, "y": 10, "w": 80, "h": 30}, "text": "hi"}));
            s.select(vec![a]);
            let before = s.doc.clone();
            match s.execute(c.id, p) {
                Ok(_) => {}
                Err(EngineError::Panicked(id)) => panic!("{id} panicked on {p}"),
                Err(_) => assert_eq!(s.doc, before, "{} with {p} failed but changed the document", c.id),
            }
        }
    }
}

#[test]
fn every_command_is_unique_and_described() {
    let mut ids: Vec<&str> = COMMANDS.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n);
    assert!(COMMANDS.iter().all(|c| !c.label.is_empty() && !c.params.is_empty() && c.id.contains('.')));
    assert_eq!(crate::describe().as_array().map(Vec::len), Some(n));
}

#[test]
fn undo_and_redo_put_back_exactly() {
    let mut s = session();
    assert!(!s.can_undo());
    let id = add(&mut s, json!({"type": "arrow", "from": {"x": 1, "y": 1}, "to": {"x": 50, "y": 50}}));
    assert_eq!(s.undo_label(), Some("Add Annotation"));
    s.execute("object.style", &json!({"ids": [id.0], "style": {"stroke": "#00FF00", "width": 9}})).unwrap();
    assert_eq!(s.doc.get(id).unwrap().style.stroke, Color::rgb(0, 255, 0));
    assert!(s.undo());
    assert_eq!(s.doc.get(id).unwrap().style.stroke, Kind::Arrow.default_style().stroke);
    assert!(s.undo());
    assert!(s.doc.objects.is_empty());
    assert!(!s.undo());
    assert!(s.redo() && s.redo());
    assert_eq!(s.doc.get(id).unwrap().style.width, 9.0);
    // A new change clears redo.
    s.undo();
    s.execute("edit.delete", &json!({"ids": [id.0]})).unwrap();
    assert!(!s.can_redo());
    assert!(s.execute("edit.redo", &json!({})).is_err(), "disabled with nothing to redo");
}

#[test]
fn selection_alone_is_not_an_undo_step() {
    let mut s = session();
    add(&mut s, json!({"type": "rect", "rect": {"x": 0, "y": 0, "w": 5, "h": 5}}));
    let undo_before = s.can_undo();
    let rev = s.revision();
    s.execute("edit.selectNone", &json!({})).unwrap();
    assert!(s.selected().is_empty());
    s.execute("edit.selectAll", &json!({})).unwrap();
    assert_eq!(s.selected().len(), 1);
    assert_eq!(s.can_undo(), undo_before);
    assert_eq!(s.revision(), rev, "the picture needn't be drawn again");
}

#[test]
fn interactive_edits_are_one_step() {
    let mut s = session();
    let id = add(&mut s, json!({"type": "rect", "rect": {"x": 0, "y": 0, "w": 5, "h": 5}}));
    s.begin("Move");
    for _ in 0..10 {
        s.doc.get_mut(id).unwrap().shape.translate(Pt::new(1.0, 0.0));
    }
    s.commit();
    assert_eq!(s.doc.get(id).unwrap().shape.rect().unwrap().x, 10.0);
    s.undo();
    assert_eq!(s.doc.get(id).unwrap().shape.rect().unwrap().x, 0.0);
    s.begin("Move");
    s.doc.get_mut(id).unwrap().shape.translate(Pt::new(5.0, 0.0));
    s.cancel();
    assert_eq!(s.doc.get(id).unwrap().shape.rect().unwrap().x, 0.0);
}

#[test]
fn picture_changes_move_the_annotations_with_it() {
    let mut s = session();
    let id = add(&mut s, json!({"type": "rect", "rect": {"x": 50, "y": 20, "w": 10, "h": 10}}));
    s.execute("image.crop", &json!({"x": 40, "y": 10, "w": 100, "h": 50})).unwrap();
    assert_eq!((s.doc.width(), s.doc.height()), (100, 50));
    assert_eq!(s.doc.get(id).unwrap().shape.rect(), Some(Rect::new(10.0, 10.0, 10.0, 10.0)));
    s.execute("image.rotate", &json!({"degrees": 90})).unwrap();
    assert_eq!((s.doc.width(), s.doc.height()), (50, 100));
    assert_eq!(s.doc.get(id).unwrap().shape.rect(), Some(Rect::new(30.0, 10.0, 10.0, 10.0)));
    s.execute("image.rotate", &json!({"degrees": -90})).unwrap();
    assert_eq!(s.doc.get(id).unwrap().shape.rect(), Some(Rect::new(10.0, 10.0, 10.0, 10.0)));
    s.execute("image.flip", &json!({"horizontal": true})).unwrap();
    assert_eq!(s.doc.get(id).unwrap().shape.rect(), Some(Rect::new(80.0, 10.0, 10.0, 10.0)));
    s.execute("image.resize", &json!({"percent": 200})).unwrap();
    assert_eq!((s.doc.width(), s.doc.height()), (200, 100));
    let r = s.doc.get(id).unwrap();
    assert_eq!(r.shape.rect(), Some(Rect::new(160.0, 20.0, 20.0, 20.0)));
    assert_eq!(r.style.width, Kind::Rect.default_style().width * 2.0);
    s.execute("effect.border", &json!({"width": 5})).unwrap();
    assert_eq!((s.doc.width(), s.doc.height()), (210, 110));
    assert_eq!(s.doc.get(id).unwrap().shape.rect().unwrap().x, 165.0);
    s.execute("image.canvas", &json!({"left": -5, "top": -5, "right": -5, "bottom": -5})).unwrap();
    assert_eq!((s.doc.width(), s.doc.height()), (200, 100));
    assert!(s.execute("image.rotate", &json!({"degrees": 45})).is_err());
    assert!(s.execute("image.crop", &json!({"x": 500, "y": 0, "w": 5, "h": 5})).is_err());
}

#[test]
fn steps_number_themselves_and_duplicates_count_on() {
    let mut s = session();
    let a = add(&mut s, json!({"type": "step", "center": {"x": 10, "y": 10}}));
    let b = add(&mut s, json!({"type": "step", "center": {"x": 30, "y": 10}}));
    let num = |s: &Session, id| match s.doc.get(id).unwrap().shape {
        Shape::Step { number, .. } => number,
        _ => 0,
    };
    assert_eq!((num(&s, a), num(&s, b)), (1, 2));
    let out = s.execute("edit.duplicate", &json!({"ids": [b.0]})).unwrap();
    let c = ObjectId(out["ids"][0].as_u64().unwrap());
    assert_eq!(num(&s, c), 3);
}

#[test]
fn text_order_flatten_and_info() {
    let mut s = session();
    let t = add(&mut s, json!({"type": "callout", "rect": {"x": 10, "y": 10, "w": 80, "h": 40}, "tail": {"x": 150, "y": 80}, "text": "a"}));
    s.execute("object.text", &json!({"id": t.0, "text": "Look here"})).unwrap();
    assert_eq!(s.doc.get(t).unwrap().shape.text(), Some("Look here"));
    let r = add(&mut s, json!({"type": "rect", "rect": {"x": 0, "y": 0, "w": 5, "h": 5}, "style": {"fill": "#000000"}}));
    s.execute("object.order", &json!({"ids": [r.0], "to": "back"})).unwrap();
    assert_eq!(s.doc.objects.first().map(|o| o.id), Some(r));
    let info = s.execute("doc.info", &json!({})).unwrap();
    assert_eq!(info["objects"].as_array().map(Vec::len), Some(2));
    s.execute("object.flatten", &json!({})).unwrap();
    assert!(s.doc.objects.is_empty());
    assert_eq!(s.doc.base.pixel(2, 2), Some(Color::BLACK));
    assert!(s.execute("object.text", &json!({"id": 1, "text": "x"})).is_err());
    assert!(s.execute("no.such", &json!({})).is_err());
}
