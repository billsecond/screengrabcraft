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

/// Steps in letters or Roman numerals count on their own beside numbered ones; a quick style or
/// the picker changes how a badge counts and its outline, keeping its place in the count.
#[test]
fn steps_count_in_letters_on_their_own_and_change_their_badge() {
    let mut s = session();
    add(&mut s, json!({"type": "step", "center": {"x": 10, "y": 10}}));
    let a = add(&mut s, json!({"type": "step", "center": {"x": 30, "y": 10}, "labels": "upper", "badge": "diamond"}));
    let b = add(&mut s, json!({"type": "step", "center": {"x": 50, "y": 10}, "labels": "upper"}));
    let label = |s: &Session, id| s.doc.get(id).unwrap().shape.step_label().unwrap();
    assert_eq!((label(&s, a), label(&s, b)), ("A".to_string(), "B".to_string()));
    let out = s.execute("edit.duplicate", &json!({"ids": [b.0]})).unwrap();
    let c = ObjectId(out["ids"][0].as_u64().unwrap());
    assert_eq!(label(&s, c), "C");
    s.execute("object.look", &json!({"ids": [a.0, b.0], "options": {"labels": "roman", "badge": "pin"}})).unwrap();
    assert_eq!((label(&s, a), label(&s, b)), ("I".to_string(), "II".to_string()));
    assert!(matches!(s.doc.get(a).unwrap().shape, Shape::Step { badge: screengrabcraft_model::BadgeShape::Pin, .. }));
    // Nonsense changes nothing.
    let before = s.doc.clone();
    assert!(s.execute("object.look", &json!({"ids": [a.0], "options": {"badge": "teapot"}})).is_err());
    assert!(s.execute("object.add", &json!({"object": {"type": "step", "center": {"x": 1, "y": 1}, "labels": 5}})).is_err());
    assert_eq!(s.doc, before);
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
    assert_eq!(
        a.shape,
        Shape::Arrow {
            from: Pt::new(100.0, 20.0),
            to: Pt::new(150.0, 60.0),
            heads: screengrabcraft_model::ArrowHeads::Both,
            bend: 0.0,
            head: Default::default()
        }
    );
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

#[test]
fn cut_out_takes_a_strip_out_and_moves_the_annotations_with_the_picture() {
    let mut s = session();
    let right = add(&mut s, json!({"type": "rect", "rect": {"x": 150, "y": 20, "w": 10, "h": 10}}));
    let inside = add(&mut s, json!({"type": "step", "center": {"x": 90, "y": 50}, "number": 1}));
    let left = add(&mut s, json!({"type": "rect", "rect": {"x": 10, "y": 20, "w": 10, "h": 10}}));
    // Columns 80 to 120 out (given either way round), with a saw's edge.
    s.execute("image.cutOut", &json!({"from": 120, "to": 80, "edge": "saw"})).unwrap();
    assert_eq!((s.doc.width(), s.doc.height()), (160, 100));
    assert_eq!(s.doc.get(right).unwrap().shape.rect().unwrap().x, 110.0);
    assert_eq!(s.doc.get(left).unwrap().shape.rect().unwrap().x, 10.0);
    assert!(matches!(s.doc.get(inside).unwrap().shape, Shape::Step { center, .. } if center.x == 80.0), "to the join");
    assert_eq!(s.undo_label(), Some("Cut Out"));
    // Rows, straight.
    s.execute("image.cutOut", &json!({"from": 0, "to": 10, "rows": true})).unwrap();
    assert_eq!((s.doc.width(), s.doc.height()), (160, 90));
    assert_eq!(s.doc.get(right).unwrap().shape.rect().unwrap().y, 10.0);
    // One undo step each.
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!((s.doc.width(), s.doc.height()), (200, 100));
    assert_eq!(s.doc.get(right).unwrap().shape.rect().unwrap().x, 150.0);
    // Nothing, everything, or a strange edge: refused, nothing changed.
    assert!(s.execute("image.cutOut", &json!({"from": 50, "to": 50})).is_err());
    assert!(s.execute("image.cutOut", &json!({"from": -10, "to": 900})).is_err());
    assert!(s.execute("image.cutOut", &json!({"from": 10, "to": 20, "edge": "zigzag"})).is_err());
    assert!(s.execute("image.cutOut", &json!({"from": 10, "to": 20, "edge": 3})).is_err());
    assert_eq!((s.doc.width(), s.doc.height()), (200, 100));
}

/// Polygons and stars come in from scripts and are kept in range; an arrow bends and stays bent
/// through a flip the other way round.
#[test]
fn polygons_stars_and_curved_arrows() {
    let mut s = session();
    let p = add(&mut s, json!({"type": "polygon", "rect": {"x": 10, "y": 10, "w": 60, "h": 50}, "sides": 5, "star": true}));
    let tri = add(&mut s, json!({"type": "polygon", "rect": {"x": 80, "y": 10, "w": 60, "h": 50}}));
    let big = add(&mut s, json!({"type": "polygon", "rect": {"x": 80, "y": 10, "w": 60, "h": 50}, "sides": 1000}));
    let shape = |s: &Session, id| s.doc.get(id).unwrap().shape.clone();
    assert!(matches!(shape(&s, p), Shape::Polygon { sides: 5, star: true, .. }));
    assert!(matches!(shape(&s, tri), Shape::Polygon { sides: 3, star: false, .. }), "a triangle unless it says");
    assert!(matches!(shape(&s, big), Shape::Polygon { sides: 12, .. }), "at most twelve");
    // A look turns a triangle into a hexagon, or into an ellipse and back.
    s.execute("object.look", &json!({"ids": [tri.0], "options": {"sides": 6}})).unwrap();
    assert!(matches!(shape(&s, tri), Shape::Polygon { sides: 6, .. }));
    s.execute("object.look", &json!({"ids": [tri.0], "options": {"round": true}})).unwrap();
    assert!(matches!(shape(&s, tri), Shape::Ellipse { .. }));
    s.execute("object.look", &json!({"ids": [tri.0], "options": {"sides": 4}})).unwrap();
    assert!(matches!(shape(&s, tri), Shape::Polygon { sides: 4, .. }));
    // A curved arrow, bent by a look, straightened, and a bend out of range kept in it.
    let a = add(&mut s, json!({"type": "arrow", "from": {"x": 10, "y": 80}, "to": {"x": 110, "y": 80}, "bend": 0.3}));
    let bend = |s: &Session, id| match s.doc.get(id).unwrap().shape {
        Shape::Arrow { bend, .. } => bend,
        _ => f32::NAN,
    };
    assert_eq!(bend(&s, a), 0.3);
    s.execute("object.look", &json!({"ids": [a.0], "options": {"bend": 0, "head": "dot"}})).unwrap();
    assert_eq!(bend(&s, a), 0.0);
    assert!(matches!(shape(&s, a), Shape::Arrow { head: screengrabcraft_model::HeadShape::Dot, .. }));
    let wild = add(&mut s, json!({"type": "arrow", "from": {"x": 10, "y": 80}, "to": {"x": 110, "y": 80}, "bend": 1e9}));
    assert_eq!(bend(&s, wild), screengrabcraft_model::object::MAX_BEND);
    // A flip mirrors the bend, so the arrow bows the way it did on the picture.
    s.execute("object.look", &json!({"ids": [a.0], "options": {"bend": 0.3}})).unwrap();
    let before = screengrabcraft_model::object::bend_middle(Pt::new(10.0, 80.0), Pt::new(110.0, 80.0), 0.3);
    s.execute("image.flip", &json!({"horizontal": true})).unwrap();
    let Shape::Arrow { from, to, bend, .. } = shape(&s, a) else { panic!("an arrow") };
    let after = screengrabcraft_model::object::bend_middle(from, to, bend);
    assert!((after.y - before.y).abs() < 0.01 && (after.x - (200.0 - before.x)).abs() < 0.01, "{before:?} {after:?}");
    let before = s.doc.clone();
    assert!(s.execute("object.add", &json!({"object": {"type": "polygon", "rect": {"x": 0, "y": 0, "w": 5, "h": 5}, "sides": "many"}})).is_err());
    assert_eq!(s.doc, before);
}

/// Every stamp can be added by its name, as `object.add`'s params say.
#[test]
fn every_stamp_is_added_by_its_name() {
    let mut s = session();
    let doc = crate::registry::find("object.add").map(|c| c.params).unwrap_or_default();
    for k in screengrabcraft_model::StampKind::ALL {
        let name = serde_json::to_value(k).unwrap();
        let name = name.as_str().unwrap();
        assert!(doc.contains(&format!("\"{name}\"")), "{name} is in object.add's params");
        let id = add(&mut s, json!({"type": "stamp", "rect": {"x": 10, "y": 10, "w": 40, "h": 40}, "stamp": name}));
        assert!(matches!(s.doc.get(id).unwrap().shape, Shape::Stamp { stamp, .. } if stamp == k));
    }
}

/// The colour effects keep to an area when given one, each in one undo step.
#[test]
fn colour_effects_change_the_whole_picture_or_an_area() {
    // Red with a black line across the area (a flat picture stays flat when blurred).
    let red = || {
        let mut r = Raster::filled(60, 40, Color::rgb(220, 30, 30)).unwrap();
        for x in 10..30 {
            let i = (15 * 60 + x) * 4;
            r.pixels_mut()[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        }
        Session::new(Document::new(r))
    };
    let area = json!({"x": 10, "y": 10, "w": 20, "h": 10});
    for (id, params) in [
        ("effect.grayscale", json!({})),
        ("effect.invert", json!({})),
        ("effect.sepia", json!({})),
        ("effect.brightness", json!({"brightness": 40})),
        ("effect.hueSaturation", json!({"hue": 120})),
        ("effect.hueSaturation", json!({"gamma": 2})),
        ("effect.blur", json!({"radius": 3})),
    ] {
        let mut s = red();
        let before = s.doc.base.clone();
        s.execute(id, &params).unwrap();
        assert_ne!(s.doc.base.pixels(), before.pixels(), "{id} on all of it");
        assert!(s.can_undo());
        let mut s = red();
        let mut with_area = params.clone();
        with_area["area"] = area.clone();
        let before = s.doc.base.clone();
        s.execute(id, &with_area).unwrap();
        assert_ne!(s.doc.base.pixel(15, 14), before.pixel(15, 14), "{id} in the area");
        assert_eq!(s.doc.base.pixel(50, 35), before.pixel(50, 35), "{id} not outside it");
        assert_eq!(s.doc.base.pixel(5, 5), before.pixel(5, 5), "{id} not outside it");
    }
    // An area off the picture, or nonsense, changes nothing.
    let mut s = red();
    assert!(s.execute("effect.sepia", &json!({"area": {"x": 100, "y": 100, "w": 5, "h": 5}})).is_err());
    assert!(s.execute("effect.sepia", &json!({"area": "all"})).is_err());
    assert!(!s.can_undo());
}

/// Perspective moves the annotations with the picture; a reflection and details below grow it.
#[test]
fn shape_effects_and_what_they_do_to_the_annotations() {
    let mut s = session();
    let a = add(&mut s, json!({"type": "rect", "rect": {"x": 150, "y": 10, "w": 40, "h": 20}}));
    let was = s.doc.get(a).unwrap().shape.rect().unwrap();
    let out = s.execute("effect.perspective", &json!({"horizontal": 60})).unwrap();
    assert!(out["width"].as_u64().unwrap() < 200);
    let now = s.doc.get(a).unwrap().shape.rect().unwrap();
    assert!(now.x < was.x && now.y > was.y, "it moved with the far side: {was:?} → {now:?}");
    for (id, params) in [
        ("effect.wavyEdge", json!({"top": true})),
        ("effect.fadedEdge", json!({})),
        ("effect.bevel", json!({})),
        ("effect.pageCurl", json!({"corner": "topLeft"})),
        ("effect.sharpen", json!({})),
    ] {
        let mut s = session();
        let before = s.doc.base.clone();
        s.execute(id, &params).unwrap();
        assert!(!s.doc.base.same_pixels(&before) && (s.doc.width(), s.doc.height()) == (200, 100), "{id}");
    }
    assert!(session().execute("effect.pageCurl", &json!({"corner": "middle"})).is_err());
    let mut s = session();
    s.execute("effect.reflection", &json!({"height": 50, "gap": 10})).unwrap();
    assert_eq!((s.doc.width(), s.doc.height()), (200, 160));
}

/// A watermark of words or of a lent picture; capture details from the document or as given.
#[test]
fn watermarks_and_capture_details() {
    let mut s = session();
    assert!(s.execute("effect.watermark", &json!({})).is_err(), "nothing to make it of");
    assert!(s.execute("effect.watermark", &json!({"picture": 0})).is_err(), "no picture lent");
    s.execute("effect.watermark", &json!({"text": "DRAFT", "color": "#FF0000", "opacity": 100, "place": "tiled", "angle": -30})).unwrap();
    assert!(s.doc.base.pixels().as_chunks::<4>().0.iter().any(|p| p[1] < 200), "drawn");
    let mut s = session();
    s.lent = vec![Raster::filled(10, 10, Color::rgb(0, 0, 255)).unwrap()];
    s.execute("effect.watermark", &json!({"picture": 0, "opacity": 100, "size": 50})).unwrap();
    assert_eq!(s.doc.base.pixel(100, 50), Some(Color::rgb(0, 0, 255)));
    assert!(s.execute("effect.watermark", &json!({"picture": 1})).is_err());
    // Details: what the document knows, or what is given.
    let mut s = session();
    s.doc.meta.captured = Some("2026-11-21T14:05:09+01:00".into());
    s.doc.meta.source = Some("Notepad".into());
    let out = s.execute("effect.captureDetails", &json!({})).unwrap();
    assert_eq!(out["text"], "2026-11-21 14:05  ·  Notepad  ·  200 × 100 pixels");
    let out = s.execute("effect.captureDetails", &json!({"time": false, "program": "A window", "size": false, "place": "below"})).unwrap();
    assert_eq!(out["text"], "A window");
    assert!(s.doc.height() > 100, "a strip below");
    let nothing = json!({"time": false, "program": false, "size": false});
    assert!(session().execute("effect.captureDetails", &nothing).is_err());
}

/// Turning, grouping, lining up, spreading out and mirroring annotations, each one undo step.
#[test]
fn annotations_are_turned_grouped_lined_up_and_mirrored() {
    let mut s = session();
    let a = add(&mut s, json!({"type": "rect", "rect": {"x": 10, "y": 10, "w": 40, "h": 20}}));
    let b = add(&mut s, json!({"type": "rect", "rect": {"x": 80, "y": 40, "w": 20, "h": 20}}));
    let c = add(&mut s, json!({"type": "arrow", "from": {"x": 150, "y": 20}, "to": {"x": 190, "y": 20}}));
    // Turned: a box keeps its turn, an arrow has its points turned.
    s.execute("object.rotate", &json!({"ids": [a], "degrees": 30})).unwrap();
    assert_eq!(s.doc.get(a).unwrap().rotation, 30.0);
    assert_eq!(s.undo_label(), Some("Rotate"));
    s.execute("object.rotate", &json!({"ids": [c], "degrees": 90})).unwrap();
    assert!(
        matches!(s.doc.get(c).unwrap().shape, Shape::Arrow { from, to, .. } if (from.x - 170.0).abs() < 0.01 && (to.y - from.y - 40.0).abs() < 0.01)
    );
    s.execute("object.rotate", &json!({"ids": [a], "angle": 0})).unwrap();
    assert_eq!(s.doc.get(a).unwrap().rotation, 0.0);
    // Grouped: selecting one selects both, and they line up as one.
    let out = s.execute("object.group", &json!({"ids": [a, b]})).unwrap();
    assert!(out["group"].as_u64().is_some());
    s.select(vec![b]);
    assert_eq!(s.selected(), vec![a, b]);
    s.execute("object.align", &json!({"ids": [a, b, c], "to": "top"})).unwrap();
    let top = |s: &Session, id| s.doc.get(id).unwrap().bounds().y;
    assert!((top(&s, a) - top(&s, c)).abs() < 0.01, "the group's top is on the arrow's");
    assert!(top(&s, b) - top(&s, a) > 25.0, "and the group kept its shape");
    // One group alone lines up on the picture.
    s.execute("object.align", &json!({"ids": [a], "to": "right"})).unwrap();
    assert!((s.doc.bounds_of(&[a, b]).unwrap().right() - 200.0).abs() < 0.01);
    // A duplicate of a group is a group of its own.
    let copies = s.execute("edit.duplicate", &json!({"ids": [a, b]})).unwrap();
    let copies: Vec<ObjectId> = copies["ids"].as_array().unwrap().iter().map(|v| ObjectId(v.as_u64().unwrap())).collect();
    let g = |s: &Session, id| s.doc.get(id).unwrap().group;
    assert!(g(&s, copies[0]).is_some() && g(&s, copies[0]) == g(&s, copies[1]) && g(&s, copies[0]) != g(&s, a));
    s.execute("object.ungroup", &json!({"ids": [a]})).unwrap();
    assert_eq!((g(&s, a), g(&s, b)), (None, None));
    // Spread out: needs three.
    assert!(s.execute("object.distribute", &json!({"ids": [a, b]})).is_err());
    s.execute("object.distribute", &json!({"ids": [a, b, c], "horizontal": false})).unwrap();
    // Mirrored across.
    let before = s.doc.get(b).unwrap().shape.rect().unwrap();
    s.execute("object.flip", &json!({"ids": [b]})).unwrap();
    assert_eq!(s.doc.get(b).unwrap().shape.rect().unwrap(), before, "a box alone mirrors onto itself");
    s.execute("object.rotate", &json!({"ids": [b], "degrees": 20})).unwrap();
    s.execute("object.flip", &json!({"ids": [b]})).unwrap();
    assert_eq!(s.doc.get(b).unwrap().rotation, -20.0);
}

/// Every annotation of a kind selected; old files (with no turns or groups) still open.
#[test]
fn every_one_of_a_kind_is_selected_and_old_annotations_still_read() {
    let mut s = session();
    let a = add(&mut s, json!({"type": "arrow", "from": {"x": 1, "y": 1}, "to": {"x": 50, "y": 50}}));
    add(&mut s, json!({"type": "rect", "rect": {"x": 10, "y": 10, "w": 40, "h": 20}}));
    let c = add(&mut s, json!({"type": "arrow", "from": {"x": 60, "y": 1}, "to": {"x": 90, "y": 50}}));
    s.select(vec![a]);
    let out = s.execute("edit.selectKind", &json!({})).unwrap();
    assert_eq!((out["selected"].as_u64(), s.selected()), (Some(2), vec![a, c]));
    assert!(!s.can_undo() || s.undo_label() != Some("Select All of a Kind"), "selecting is no undo step");
    s.execute("edit.selectKind", &json!({"kind": "rect"})).unwrap();
    assert_eq!(s.selected().len(), 1);
    assert!(s.execute("edit.selectKind", &json!({"kind": "teapot"})).is_err());
    let old: screengrabcraft_model::Object =
        serde_json::from_value(json!({"id": 4, "type": "rect", "rect": {"x": 0, "y": 0, "w": 5, "h": 5}})).unwrap();
    assert_eq!((old.rotation, old.group), (0.0, None));
    // Written without them when there are none, with them when there are.
    let v = serde_json::to_value(&old).unwrap();
    assert!(v.get("rotation").is_none() && v.get("group").is_none());
    let turned = screengrabcraft_model::Object { rotation: 12.5, group: Some(3), ..old };
    let v = serde_json::to_value(&turned).unwrap();
    assert_eq!((v["rotation"].as_f64(), v["group"].as_u64()), (Some(12.5), Some(3)));
}

/// An ellipse, a freehand outline, the magic wand and an inverted area are moved, deleted and
/// cropped as their shapes; the eraser rubs out what it passes over.
#[test]
fn shaped_areas_the_magic_wand_and_the_eraser() {
    // White with a blue square in the middle.
    let blue_square = || {
        let mut r = Raster::filled(100, 80, Color::WHITE).unwrap();
        for y in 30..50 {
            for x in 40..60 {
                let i = (y * 100 + x) * 4;
                r.pixels_mut()[i..i + 4].copy_from_slice(&[0, 0, 255, 255]);
            }
        }
        Session::new(Document::new(r))
    };
    let alpha = |s: &Session, x, y| s.doc.base.pixel(x, y).map_or(0, |c| c.a);
    // An ellipse: its box's corners stay.
    let mut s = blue_square();
    s.execute("image.erase", &json!({"x": 10, "y": 10, "w": 40, "h": 30, "shape": "ellipse"})).unwrap();
    assert_eq!((alpha(&s, 30, 25), alpha(&s, 11, 11)), (0, 255));
    // The wand takes the blue square, joined to the pixel, and lifts it out as a piece.
    let mut s = blue_square();
    let out = s.execute("image.lift", &json!({"shape": "wand", "seed": {"x": 45, "y": 35}, "tolerance": 10})).unwrap();
    let id = ObjectId(out["id"].as_u64().unwrap());
    assert_eq!(s.doc.get(id).unwrap().shape.rect(), Some(Rect::new(40.0, 30.0, 20.0, 20.0)));
    assert_eq!((alpha(&s, 45, 35), alpha(&s, 10, 10)), (0, 255));
    // Inverted: everything but the square goes.
    let mut s = blue_square();
    s.execute("image.erase", &json!({"shape": "wand", "seed": {"x": 45, "y": 35}, "invert": true})).unwrap();
    assert_eq!((alpha(&s, 45, 35), alpha(&s, 10, 10)), (255, 0));
    // A freehand triangle cropped to: the box round it, see-through outside it.
    let mut s = blue_square();
    let tri = json!({"shape": "freehand", "points": [{"x": 0, "y": 0}, {"x": 60, "y": 0}, {"x": 0, "y": 60}]});
    let out = s.execute("image.crop", &tri).unwrap();
    assert!(out["width"].as_u64().unwrap() <= 62 && out["height"].as_u64().unwrap() <= 62);
    assert_eq!((alpha(&s, 5, 5), alpha(&s, 55, 55)), (255, 0));
    // A colour change kept to an ellipse.
    let mut s = blue_square();
    s.execute("effect.invert", &json!({"area": {"x": 0, "y": 0, "w": 100, "h": 80, "shape": "ellipse"}})).unwrap();
    assert_eq!((s.doc.base.pixel(50, 5), s.doc.base.pixel(1, 1)), (Some(Color::BLACK), Some(Color::WHITE)));
    // Nonsense changes nothing.
    let mut s = blue_square();
    assert!(s.execute("image.erase", &json!({"shape": "teapot", "x": 0, "y": 0, "w": 5, "h": 5})).is_err());
    assert!(s.execute("image.erase", &json!({"shape": "wand", "seed": {"x": -5, "y": 0}})).is_err());
    assert!(!s.can_undo());
    // The eraser: see-through along the stroke, or a colour.
    let mut s = blue_square();
    s.execute("image.eraseStroke", &json!({"points": [{"x": 5, "y": 70}, {"x": 95, "y": 70}], "width": 8})).unwrap();
    assert_eq!((alpha(&s, 50, 70), alpha(&s, 50, 60)), (0, 255));
    s.execute("image.eraseStroke", &json!({"points": [{"x": 50, "y": 40}], "width": 10, "color": "#FF0000"})).unwrap();
    assert_eq!(s.doc.base.pixel(50, 40), Some(Color::rgb(255, 0, 0)));
    assert_eq!(s.undo_label(), Some("Erase"));
    assert!(s.execute("image.eraseStroke", &json!({"points": [{"x": 500, "y": 500}]})).is_err());
}

/// Pictures combined into one, or laid out in a template of annotations; one undo step each.
#[test]
fn pictures_are_combined_or_laid_out_in_a_template() {
    let mut s = session();
    let a = add(&mut s, json!({"type": "rect", "rect": {"x": 10, "y": 10, "w": 50, "h": 30}}));
    assert!(s.execute("image.combine", &json!({})).is_err(), "nothing lent to combine with");
    s.lent = vec![Raster::filled(80, 60, Color::rgb(0, 0, 255)).unwrap(), Raster::filled(40, 140, Color::rgb(0, 255, 0)).unwrap()];
    let out = s.execute("image.combine", &json!({"layout": "across", "gap": 10, "background": "#000000"})).unwrap();
    assert_eq!((out["width"].as_u64(), out["height"].as_u64()), (Some(10 + 200 + 10 + 80 + 10 + 40 + 10), Some(160)));
    // This picture first, centred in its row: its annotations moved with it.
    assert_eq!((s.doc.base.pixel(15, 35), s.doc.base.pixel(15, 25)), (Some(Color::WHITE), Some(Color::BLACK)));
    assert_eq!(s.doc.base.pixel(250, 75), Some(Color::rgb(0, 0, 255)));
    assert_eq!(s.doc.get(a).unwrap().shape.rect().unwrap().min(), Pt::new(20.0, 40.0));
    assert_eq!(s.undo_label(), Some("Combine Pictures"));
    s.undo();
    s.execute("image.combine", &json!({"layout": "grid", "columns": 2})).unwrap();
    assert!(s.doc.height() > 200);
    assert!(s.execute("image.combine", &json!({"layout": "sideways"})).is_err());
    // A template: a page of annotations (the pictures, the title, the steps).
    let mut s = session();
    s.lent = vec![Raster::filled(80, 60, Color::rgb(0, 0, 255)).unwrap()];
    s.execute("image.template", &json!({"title": "How to", "steps": ["Open it", "Click it"]})).unwrap();
    assert_eq!(s.doc.count(Kind::Image), 2);
    assert_eq!(s.doc.count(Kind::Step), 2);
    assert_eq!(s.doc.count(Kind::Text), 3, "the title and two steps");
    for layout in ["stepsDown", "beforeAfter", "grid"] {
        let mut t = session();
        t.lent = vec![Raster::filled(80, 60, Color::rgb(0, 0, 255)).unwrap()];
        t.execute("image.template", &json!({"layout": layout})).unwrap();
        assert!(t.doc.count(Kind::Image) == 2, "{layout}");
    }
    assert!(session().execute("image.template", &json!({"layout": "beforeAfter"})).is_err(), "before and after needs two");
    s.undo();
    assert_eq!((s.doc.width(), s.doc.height()), (200, 100), "undo puts the picture back");
}
