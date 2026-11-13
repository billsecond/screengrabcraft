use screengrabcraft_model::{Color, Document, Kind, ObjectId, Pt, Raster, Rect, Shape};
use screengrabcraft_render::matte::{Matte, MatteError};
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

fn area(s: &Session, id: ObjectId) -> Rect {
    s.doc.get(id).map(|o| o.shape.bounds(&o.style)).unwrap()
}

#[test]
fn a_nudge_past_the_edge_grows_the_picture_to_hold_it() {
    let mut s = session();
    let (w, h) = (s.doc.width(), s.doc.height());
    let a = add(&mut s, json!({"type": "rect", "rect": {"x": 150, "y": 10, "w": 40, "h": 30}}));
    s.select(vec![a]);
    s.execute("edit.nudge", &json!({"dx": 100, "dy": 100})).unwrap();
    assert!(s.doc.bounds().encloses(area(&s, a)), "{:?}", area(&s, a));
    assert!(s.doc.width() > w + 80 && s.doc.height() > h + 30, "the picture grew: {} x {}", s.doc.width(), s.doc.height());
    assert_eq!(s.doc.base.pixel(s.doc.width() - 1, s.doc.height() - 1).map(|c| c.a), Some(0), "see-through where it is new");
    assert_eq!(s.doc.base.pixel(5, 5).map(|c| c.a), Some(255), "the picture itself is as it was");
    // However far it is asked to go, the picture only grows so far.
    s.execute("edit.nudge", &json!({"dx": 1e9, "dy": 0})).unwrap();
    assert!(s.doc.width() < w + 20_000);
    // Undone, the picture is its own size again.
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!((s.doc.width(), s.doc.height()), (w, h));
}

#[test]
fn a_nudged_group_keeps_its_shape_at_the_edge() {
    let mut s = session();
    let a = add(&mut s, json!({"type": "rect", "rect": {"x": 150, "y": 10, "w": 20, "h": 20}}));
    let b = add(&mut s, json!({"type": "rect", "rect": {"x": 100, "y": 10, "w": 20, "h": 20}}));
    s.select(vec![a, b]);
    let gap = area(&s, a).x - area(&s, b).x;
    s.execute("edit.nudge", &json!({"dx": 500})).unwrap();
    assert!(s.doc.bounds().encloses(area(&s, a)));
    assert!((area(&s, a).x - area(&s, b).x - gap).abs() < 0.01, "both moved the same");
}

#[test]
fn an_annotation_off_the_picture_is_held_by_it_once_it_is_nudged() {
    let mut s = session();
    let w = s.doc.width();
    let a = add(&mut s, json!({"type": "rect", "rect": {"x": -50, "y": 10, "w": 40, "h": 30}}));
    s.select(vec![a]);
    s.execute("edit.nudge", &json!({"dx": -5})).unwrap();
    assert!(s.doc.bounds().encloses(area(&s, a)), "{:?}", area(&s, a));
    assert!(s.doc.width() >= w + 55, "it grew to the left by as much as was off it: {}", s.doc.width());
    assert_eq!(s.doc.base.pixel(0, 0).map(|c| c.a), Some(0));
}

#[test]
fn a_duplicate_stays_on_the_picture_and_off_its_original() {
    let mut s = session();
    let a = add(&mut s, json!({"type": "rect", "rect": {"x": 150, "y": 60, "w": 45, "h": 35}}));
    s.select(vec![a]);
    s.execute("edit.duplicate", &json!({})).unwrap();
    let copy = *s.selection.first().unwrap();
    assert_ne!(copy, a);
    assert!(s.doc.bounds().encloses(area(&s, copy)), "{:?}", area(&s, copy));
    assert!((area(&s, copy).x - area(&s, a).x).abs() >= 16.0, "against the edge it goes the other way");
    assert!((area(&s, copy).y - area(&s, a).y).abs() >= 16.0);
}

#[test]
fn a_duplicate_with_room_is_where_it_always_was() {
    let mut s = session();
    let a = add(&mut s, json!({"type": "rect", "rect": {"x": 10, "y": 10, "w": 20, "h": 20}}));
    s.select(vec![a]);
    s.execute("edit.duplicate", &json!({})).unwrap();
    let copy = *s.selection.first().unwrap();
    assert!((area(&s, copy).x - area(&s, a).x - 16.0).abs() < 0.01);
    assert!((area(&s, copy).y - area(&s, a).y - 16.0).abs() < 0.01);
}

/// A blue page with a red square in the middle.
fn square_on_blue() -> Session {
    let mut r = Raster::filled(60, 40, Color::rgb(30, 90, 200)).unwrap();
    for y in 10..30 {
        for x in 20..40 {
            let i = (y * 60 + x) * 4;
            r.pixels_mut()[i..i + 4].copy_from_slice(&[229, 57, 53, 255]);
        }
    }
    Session::new(Document::new(r))
}

#[test]
fn a_solid_background_goes_in_one_undo_step() {
    let mut s = square_on_blue();
    let before = s.doc.base.clone();
    let out = s.execute("image.removeSolidBackground", &json!({})).unwrap();
    assert_eq!(out["color"], "#1E5AC8");
    assert_eq!(out["cleared"], 60 * 40 - 400);
    assert_eq!(s.doc.base.pixel(0, 0).map(|c| c.a), Some(0));
    assert_eq!(s.doc.base.pixel(30, 20), Some(Color::rgb(229, 57, 53)));
    assert_eq!(s.undo_label(), Some("Remove Solid Background"));
    assert!(s.undo());
    assert_eq!(s.doc.base, before);
    // A colour of its own, and how close is close enough.
    s.execute("image.removeSolidBackground", &json!({"color": "#1E5AC8", "tolerance": 0})).unwrap();
    assert_eq!(s.doc.base.pixel(0, 0).map(|c| c.a), Some(0));
}

#[test]
fn nothing_to_take_away_is_an_error_that_changes_nothing() {
    // All one colour: nothing would be left.
    let mut s = session();
    assert!(matches!(s.execute("image.removeSolidBackground", &json!({})), Err(EngineError::Failed(_))));
    assert!(!s.can_undo() && s.doc.base.pixel(5, 5) == Some(Color::WHITE));
    // A colour that isn't there.
    let mut s = square_on_blue();
    assert!(s.execute("image.removeSolidBackground", &json!({"color": "#00FF00", "tolerance": 3})).is_err());
    assert!(s.execute("image.makeColorTransparent", &json!({"color": "#00FF00", "tolerance": 3})).is_err());
    assert!(!s.can_undo());
    // A picture of one pixel.
    let mut one = Session::new(Document::new(Raster::single(Color::RED)));
    assert!(one.execute("image.removeSolidBackground", &json!({})).is_err());
    assert!(one.execute("image.makeColorTransparent", &json!({"x": 0, "y": 0})).is_err());
    assert!(one.execute("image.makeColorTransparent", &json!({"color": "#000000"})).is_err());
}

#[test]
fn a_colour_goes_everywhere_or_from_a_pixel() {
    let mut s = square_on_blue();
    s.execute("image.makeColorTransparent", &json!({"x": 30, "y": 20})).unwrap();
    assert_eq!(s.doc.base.pixel(30, 20).map(|c| c.a), Some(0), "the red square went");
    assert_eq!(s.doc.base.pixel(0, 0).map(|c| c.a), Some(255), "the blue stayed");
    s.undo();
    s.execute("image.makeColorTransparent", &json!({"color": "#1E5AC8", "contiguous": true, "x": 0, "y": 0})).unwrap();
    assert_eq!(s.doc.base.pixel(59, 39).map(|c| c.a), Some(0));
    for bad in [
        json!({}),
        json!({"x": 5}),
        json!({"x": 500, "y": 5}),
        json!({"x": "left", "y": 0}),
        json!({"color": "#1E5AC8", "contiguous": true}),
        json!({"color": 12}),
    ] {
        let before = s.doc.clone();
        assert!(s.execute("image.makeColorTransparent", &bad).is_err(), "{bad}");
        assert_eq!(s.doc, before);
    }
}

#[test]
fn a_fill_colours_one_area_in_one_undo_step() {
    let mut s = square_on_blue();
    let out = s.execute("image.fill", &json!({"x": 30, "y": 20, "color": "#FFD600"})).unwrap();
    assert_eq!(out["filled"], 20 * 20, "the red square");
    assert_eq!(s.doc.base.pixel(30, 20), Some(Color::rgb(0xFF, 0xD6, 0x00)));
    assert_eq!(s.doc.base.pixel(0, 0), Some(Color::rgb(30, 90, 200)), "the blue round it stays");
    assert_eq!(s.undo_label(), Some("Fill"));
    // Filling it with the colour it already is changes nothing, and adds no undo step.
    let out = s.execute("image.fill", &json!({"x": 30, "y": 20, "color": "#FFD600"})).unwrap();
    assert_eq!(out["filled"], 0);
    s.undo();
    assert_eq!(s.doc.base.pixel(30, 20), Some(Color::rgb(229, 57, 53)));
    assert!(!s.can_undo(), "one fill, one step");
    // The tolerance reaches the blue too.
    s.execute("image.fill", &json!({"x": 30, "y": 20, "color": "#00000000", "tolerance": 255})).unwrap();
    assert_eq!(s.doc.base.pixel(0, 0).map(|c| c.a), Some(0));
    for bad in [
        json!({}),
        json!({"x": 5, "y": 5}),
        json!({"x": 500, "y": 5, "color": "#FFFFFF"}),
        json!({"x": -1, "y": 5, "color": "#FFFFFF"}),
        json!({"x": "left", "y": 0, "color": "#FFFFFF"}),
        json!({"x": 1, "y": 1, "color": 12}),
        json!({"x": 1, "y": 1, "color": "yellow"}),
        json!({"x": 1, "y": 1, "color": "#FFFFFF", "tolerance": "lots"}),
    ] {
        let before = s.doc.clone();
        assert!(s.execute("image.fill", &bad).is_err(), "{bad}");
        assert_eq!(s.doc, before);
    }
}

/// A model that calls the left half of any picture background.
struct LeftIsBackground;

impl screengrabcraft_render::matte::Matting for LeftIsBackground {
    fn name(&self) -> &str {
        "halves"
    }

    fn matte(&self, picture: &Raster, progress: &mut dyn FnMut(f32) -> bool) -> Result<Matte, MatteError> {
        if !progress(0.5) {
            return Err(MatteError::Cancelled);
        }
        let (w, h) = (picture.width(), picture.height());
        let alpha = (0..h).flat_map(|_| (0..w).map(move |x| if x < w / 2 { 0 } else { 255 })).collect();
        Ok(Matte::new(w, h, alpha)?)
    }
}

/// A model that falls over.
struct Broken;

impl screengrabcraft_render::matte::Matting for Broken {
    fn name(&self) -> &str {
        "broken"
    }

    fn matte(&self, _: &Raster, _: &mut dyn FnMut(f32) -> bool) -> Result<Matte, MatteError> {
        panic!("a model that falls over")
    }
}

#[test]
fn the_background_goes_with_a_model() {
    let mut s = session();
    assert!(matches!(s.execute("image.removeBackground", &json!({})), Err(EngineError::Failed(_))), "no model, no change");
    s.background.set_model(Some(std::sync::Arc::new(LeftIsBackground)));
    assert!(matches!(s.execute("image.removeBackground", &json!({"model": "other"})), Err(EngineError::Params(_))));
    let out = s.execute("image.removeBackground", &json!({"model": "halves"})).unwrap();
    assert_eq!(out["model"], "halves");
    assert_eq!(s.doc.base.pixel(10, 50).map(|c| c.a), Some(0));
    assert_eq!(s.doc.base.pixel(190, 50).map(|c| c.a), Some(255));
    assert_eq!(s.undo_label(), Some("Remove Background"));
    assert!(s.undo());
    assert_eq!(s.doc.base.pixel(10, 50).map(|c| c.a), Some(255));
}

#[test]
fn a_matte_worked_out_ahead_is_used_for_its_picture_only() {
    let mut s = session();
    let matte = Matte::new(200, 100, vec![0; 200 * 100]).unwrap();
    // Worked out for the picture as it is: used, without a model.
    s.background.prepare(s.doc.base.clone(), "ahead", matte.clone());
    assert_eq!(s.execute("image.removeBackground", &json!({})).unwrap()["model"], "ahead");
    assert_eq!(s.doc.base.pixel(100, 50).map(|c| c.a), Some(0));
    s.undo();
    // Worked out for a picture that has changed since: not used.
    s.background.prepare(s.doc.base.clone(), "ahead", matte);
    s.execute("effect.invert", &json!({})).unwrap();
    assert!(s.execute("image.removeBackground", &json!({})).is_err());
    assert_eq!(s.doc.base.pixel(100, 50).map(|c| c.a), Some(255));
}

#[test]
fn a_model_that_falls_over_changes_nothing() {
    let mut s = session();
    s.background.set_model(Some(std::sync::Arc::new(Broken)));
    let before = s.doc.clone();
    assert!(matches!(s.execute("image.removeBackground", &json!({})), Err(EngineError::Panicked(_))));
    assert_eq!(s.doc, before);
    assert!(!s.can_undo());
}

#[test]
fn a_script_places_an_annotation_where_it_says() {
    let mut s = session();
    let a = add(&mut s, json!({"type": "rect", "rect": {"x": 500, "y": 500, "w": 20, "h": 20}}));
    assert_eq!(s.doc.get(a).and_then(|o| o.shape.rect()), Some(Rect::new(500.0, 500.0, 20.0, 20.0)));
}

#[test]
fn a_quick_style_changes_the_selection_in_one_undo_step() {
    let mut s = session();
    let callout = add(&mut s, json!({"type": "callout", "rect": {"x": 20, "y": 10, "w": 60, "h": 30}, "tail": {"x": 30, "y": 60}, "text": "hi"}));
    let arrow = add(&mut s, json!({"type": "arrow", "from": {"x": 100, "y": 20}, "to": {"x": 150, "y": 60}}));
    let size = s.doc.get(callout).unwrap().style.font_size;
    s.select(vec![callout, arrow]);
    let params = json!({
        "style": {"fill": "#1E88E5", "stroke": "#155FA0", "fontSize": 80},
        "options": {"form": "arrow", "tail": "right", "heads": "both"}
    });
    s.execute("object.look", &params).unwrap();
    let c = s.doc.get(callout).unwrap();
    let Shape::Callout { form, tail, rect, .. } = c.shape.clone() else { panic!("a callout") };
    assert_eq!(form, screengrabcraft_model::CalloutForm::Arrow);
    assert!(tail.x > rect.right() && (tail.y - rect.center().y).abs() < 0.5, "the tail points right: {tail:?} {rect:?}");
    assert_eq!(c.style.fill, Color::rgb(0x1E, 0x88, 0xE5));
    assert_eq!(c.style.font_size, size, "the text keeps its size");
    let a = s.doc.get(arrow).unwrap();
    assert_eq!(a.shape, Shape::Arrow { from: Pt::new(100.0, 20.0), to: Pt::new(150.0, 60.0), heads: screengrabcraft_model::ArrowHeads::Both });
    assert_eq!(s.undo_label(), Some("Apply Quick Style"));
    assert!(s.undo());
    assert!(matches!(s.doc.get(callout).unwrap().shape, Shape::Callout { form: screengrabcraft_model::CalloutForm::Rounded, .. }));
    let heads = |s: &Session| match s.doc.get(arrow).map(|o| &o.shape) {
        Some(Shape::Arrow { heads, .. }) => Some(*heads),
        _ => None,
    };
    assert_eq!(heads(&s), Some(screengrabcraft_model::ArrowHeads::End), "one step for both");
    // The size too when asked; a rectangle made round is an ellipse; a tail off the picture is
    // brought back onto it.
    s.select(vec![callout]);
    s.execute("object.look", &json!({"style": {"fontSize": 40}, "keepTextSize": false, "options": {"tail": "up"}})).unwrap();
    let c = s.doc.get(callout).unwrap();
    assert_eq!(c.style.font_size, 40.0);
    let Shape::Callout { tail, .. } = c.shape else { panic!("a callout") };
    assert!(tail.y >= 0.0, "on the picture: {tail:?}");
    let r = add(&mut s, json!({"type": "rect", "rect": {"x": 10, "y": 50, "w": 40, "h": 30}}));
    s.execute("object.look", &json!({"ids": [r.0], "options": {"round": true}})).unwrap();
    assert_eq!(s.doc.get(r).unwrap().shape.kind(), Kind::Ellipse);
    // Nonsense is an error that changes nothing.
    let before = s.doc.clone();
    for bad in [json!({"options": {"form": "teapot"}}), json!({"style": 4}), json!({"keepTextSize": "yes"}), json!({"options": 7})] {
        assert!(s.execute("object.look", &bad).is_err(), "{bad}");
        assert_eq!(s.doc, before);
    }
}

#[test]
fn a_title_look_makes_its_text_bigger_and_its_box_taller() {
    let mut s = session();
    let t = add(&mut s, json!({"type": "text", "rect": {"x": 10, "y": 10, "w": 180, "h": 30}, "text": "Title", "style": {"fontSize": 20}}));
    s.select(vec![t]);
    let before = s.doc.get(t).unwrap().shape.rect().unwrap();
    s.execute("object.look", &json!({"style": {"fontSize": 56, "textOutline": "#FFFFFF", "textOutlineWidth": 4}, "keepTextSize": false})).unwrap();
    let o = s.doc.get(t).unwrap();
    assert_eq!((o.style.font_size, o.style.text_outline, o.style.text_outline_width), (56.0, Color::WHITE, 4.0));
    let after = o.shape.rect().unwrap();
    assert!(after.h > before.h && after.w == before.w, "{before:?} → {after:?}");
    assert!(s.doc.bounds().encloses(o.shape.bounds(&o.style)), "still on the picture");
}

#[test]
fn an_area_lifted_out_moves_as_a_picture_and_leaves_a_see_through_hole() {
    let mut s = square_on_blue();
    // The red square (20 × 20 at 20, 10) with a little blue round it.
    let out = s.execute("image.lift", &json!({"x": 15, "y": 5, "w": 30, "h": 30})).unwrap();
    let id = ObjectId(out["id"].as_u64().unwrap());
    assert_eq!(s.selected(), vec![id], "the piece is selected, to be moved");
    assert_eq!(s.doc.base.pixel(30, 20).map(|c| c.a), Some(0), "the hole is see-through");
    assert_eq!(s.doc.base.pixel(0, 0), Some(Color::rgb(30, 90, 200)), "the rest stays");
    let o = s.doc.get(id).unwrap();
    assert_eq!(o.shape.rect(), Some(Rect::new(15.0, 5.0, 30.0, 30.0)), "where it was");
    assert!(!o.style.shadow, "looking as it did");
    // Drawn where it was, the picture looks the same.
    let shown = screengrabcraft_render::render(&s.doc).unwrap();
    assert_eq!(shown.pixel(30, 20), Some(Color::rgb(229, 57, 53)));
    assert_eq!(s.undo_label(), Some("Move Area"));
    s.undo();
    assert_eq!(s.doc.base.pixel(30, 20), Some(Color::rgb(229, 57, 53)), "one undo puts it back");
    assert!(s.doc.objects.is_empty() && s.doc.images.is_empty());
    for bad in
        [json!({}), json!({"x": 500, "y": 5, "w": 10, "h": 10}), json!({"x": 1, "y": 1, "w": 0, "h": 10}), json!({"x": "a", "y": 1, "w": 5, "h": 5})]
    {
        let before = s.doc.clone();
        assert!(s.execute("image.lift", &bad).is_err(), "{bad}");
        assert_eq!(s.doc, before);
    }
}

#[test]
fn an_area_erased_is_see_through_and_only_that_area() {
    let mut s = square_on_blue();
    s.execute("image.erase", &json!({"x": 20, "y": 10, "w": 10, "h": 10})).unwrap();
    assert_eq!(s.doc.base.pixel(25, 15).map(|c| c.a), Some(0));
    assert_eq!(s.doc.base.pixel(35, 25), Some(Color::rgb(229, 57, 53)), "the square's other part stays");
    // An area reaching past the edge is cut to the picture.
    s.execute("image.erase", &json!({"x": -50, "y": -50, "w": 60, "h": 60})).unwrap();
    assert_eq!(s.doc.base.pixel(0, 0).map(|c| c.a), Some(0));
    assert!(s.execute("image.erase", &json!({"x": 900, "y": 900, "w": 5, "h": 5})).is_err());
}

#[test]
fn flatten_draws_only_the_annotations_given_or_all() {
    let mut s = session();
    let a = add(&mut s, json!({"type": "rect", "rect": {"x": 10, "y": 10, "w": 40, "h": 40}}));
    let b = add(&mut s, json!({"type": "rect", "rect": {"x": 100, "y": 10, "w": 40, "h": 40}}));
    for (id, c) in [(a, Color::rgb(255, 0, 0)), (b, Color::rgb(0, 0, 255))] {
        let o = s.doc.get_mut(id).unwrap();
        o.style.fill = c;
        o.style.stroke = c;
        o.style.shadow = false;
    }
    s.execute("object.flatten", &json!({"ids": [a.0]})).unwrap();
    assert!(s.doc.get(a).is_none() && s.doc.get(b).is_some(), "only the one given goes into the picture");
    assert_eq!(s.doc.base.pixel(30, 30), Some(Color::rgb(255, 0, 0)), "drawn into the picture");
    assert_eq!(s.doc.base.pixel(120, 30), Some(Color::WHITE), "the other is still an annotation");
    s.execute("object.flatten", &json!({})).unwrap();
    assert!(s.doc.objects.is_empty());
    assert_eq!(s.doc.base.pixel(120, 30), Some(Color::rgb(0, 0, 255)));
    assert!(s.execute("object.flatten", &json!({"ids": [999]})).is_err(), "no such annotation");
}
