//! Polygons of clicked points, bent lines and callouts with several tails, driven headless the
//! way a person uses them.

use screengrabcraft_model::{CalloutForm, Kind, Pt, Rect, Shape};
use serde_json::json;

use crate::editor::Tool;
use crate::quick_tests::shot;
use crate::tests::{at, click_at, drag, editor, ready};

#[test]
fn a_polygon_is_drawn_by_clicking_its_corners() {
    let mut h = ready(editor(), Tool::Draw(Kind::Polygon));
    h.state_mut().polygon_points = true;
    for (x, y) in [(60.0, 60.0), (300.0, 80.0), (260.0, 240.0), (90.0, 200.0)] {
        let p = at(&h, x, y);
        click_at(&mut h, p);
    }
    assert_eq!(h.state().polygon_draft.len(), 4);
    let hover = at(&h, 120.0, 120.0);
    h.hover_at(hover);
    h.run_steps(2);
    shot(&mut h, "polygon-clicking");
    // Enter closes it.
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let doc = &h.state().session.doc;
    let Some(Shape::Polygon { rect, corners, .. }) = doc.objects.first().map(|o| o.shape.clone()) else { panic!("a polygon") };
    assert_eq!(corners.len(), 4);
    assert!((rect.x - 60.0).abs() < 1.5 && (rect.right() - 300.0).abs() < 1.5 && (rect.bottom() - 240.0).abs() < 1.5, "{rect:?}");
    assert!(h.state().polygon_draft.is_empty());
    shot(&mut h, "polygon-clicked");
    // Three corners, then a click on the first: closed too. Esc lets a polygon under way go.
    for (x, y) in [(40.0, 260.0), (120.0, 260.0), (80.0, 220.0)] {
        let p = at(&h, x, y);
        click_at(&mut h, p);
    }
    let first = at(&h, 40.0, 260.0);
    click_at(&mut h, first);
    assert_eq!(h.state().session.doc.objects.len(), 2);
    for (x, y) in [(200.0, 20.0), (240.0, 20.0)] {
        let p = at(&h, x, y);
        click_at(&mut h, p);
    }
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().polygon_draft.is_empty());
    assert_eq!(h.state().session.doc.objects.len(), 2);
}

#[test]
fn a_line_is_bent_by_its_middle() {
    let mut h = ready(editor(), Tool::Select);
    let id = {
        let ed = h.state_mut();
        let id =
            ed.session.doc.add(Shape::Line { from: Pt::new(60.0, 150.0), to: Pt::new(340.0, 150.0), bend: 0.0 }, Kind::Line.default_style()).unwrap();
        ed.session.select(vec![id]);
        id
    };
    h.run_steps(2);
    let (a, b) = (at(&h, 200.0, 150.0), at(&h, 200.0, 90.0));
    drag(&mut h, a, b);
    let Some(Shape::Line { bend, .. }) = h.state().session.doc.get(id).map(|o| o.shape.clone()) else { panic!("a line") };
    assert!(bend > 0.15, "{bend}");
    shot(&mut h, "line-bent");
}

#[test]
fn a_callout_gets_more_tails_each_dragged_on_its_own() {
    let mut h = ready(editor(), Tool::Select);
    let id = {
        let ed = h.state_mut();
        let shape = Shape::Callout {
            rect: Rect::new(130.0, 40.0, 150.0, 60.0),
            tail: Pt::new(120.0, 200.0),
            text: "Both of these".into(),
            form: CalloutForm::Rounded,
            tails: Vec::new(),
        };
        let id = ed.session.doc.add(shape, Kind::Callout.default_style()).unwrap();
        ed.session.select(vec![id]);
        id
    };
    h.run_steps(2);
    let ctx = h.ctx.clone();
    h.state_mut().run(&ctx, "object.addTail", json!({}));
    h.run_steps(2);
    // The new tail is across from the first; dragged, only it moves.
    let (a, b) = (at(&h, 290.0, 200.0), at(&h, 330.0, 250.0));
    drag(&mut h, a, b);
    let Some(Shape::Callout { tail, tails, .. }) = h.state().session.doc.get(id).map(|o| o.shape.clone()) else { panic!("a callout") };
    assert_eq!(tail, Pt::new(120.0, 200.0));
    assert!(tails.first().is_some_and(|t| (t.x - 330.0).abs() < 2.0 && (t.y - 250.0).abs() < 2.0), "{tails:?}");
    shot(&mut h, "callout-two-tails");
}
