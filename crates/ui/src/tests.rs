//! The editor driven headless, the way a person uses it: tools, drags, typing, crop, undo.

use egui::{Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use screengrabcraft_model::{Color, Document, EditorSettings, ImageFormat, Kind, Raster, Shape};

use crate::editor::{Editor, Tool};
use crate::services::{NoServices, OpenedFile};

fn editor() -> Editor {
    let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
    ed.session.replace(Document::new(Raster::filled(400, 300, Color::WHITE).unwrap()));
    ed.view.fit_pending = true;
    ed
}

fn harness(ed: Editor) -> Harness<'static, Editor> {
    Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    )
}

/// Where a picture point is on the screen.
fn at(h: &Harness<'_, Editor>, x: f32, y: f32) -> Pos2 {
    h.state().view.to_screen(screengrabcraft_model::Pt::new(x, y))
}

fn drag(h: &mut Harness<'_, Editor>, from: Pos2, to: Pos2) {
    h.hover_at(from);
    h.run_steps(2);
    h.drag_at(from);
    h.run_steps(2);
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        h.hover_at(pos2(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t));
        h.run_steps(1);
    }
    h.drop_at(to);
    h.run_steps(3);
}

#[test]
fn draws_an_arrow_and_undoes_it() {
    let mut h = harness(editor());
    h.run_steps(4);
    h.state_mut().set_tool(Tool::Draw(Kind::Arrow));
    let (a, b) = (at(&h, 50.0, 50.0), at(&h, 250.0, 150.0));
    drag(&mut h, a, b);
    let ed = h.state();
    assert_eq!(ed.session.doc.objects.len(), 1, "one arrow");
    let Some(Shape::Arrow { from, to, .. }) = ed.session.doc.objects.first().map(|o| o.shape.clone()) else { panic!("not an arrow") };
    assert!(from.distance(screengrabcraft_model::Pt::new(50.0, 50.0)) < 3.0, "{from:?}");
    assert!(to.distance(screengrabcraft_model::Pt::new(250.0, 150.0)) < 3.0, "{to:?}");
    assert_eq!(ed.session.undo_label(), Some("Draw Arrow"));
    assert!(h.state_mut().session.undo());
    assert!(h.state().session.doc.objects.is_empty());
}

#[test]
fn every_drawing_tool_draws_something() {
    for (tool, _, _, _) in Tool::BAR {
        let Tool::Draw(kind) = tool else { continue };
        let mut h = harness(editor());
        h.run_steps(4);
        h.state_mut().set_tool(tool);
        let (a, b) = (at(&h, 60.0, 60.0), at(&h, 200.0, 160.0));
        drag(&mut h, a, b);
        let n = h.state().session.doc.objects.len();
        assert_eq!(n, 1, "{kind:?} drew {n} annotations");
        assert_eq!(h.state().session.doc.objects.first().map(|o| o.shape.kind()), Some(kind));
    }
}

#[test]
fn typing_into_a_new_text_box() {
    let mut h = harness(editor());
    h.run_steps(4);
    h.state_mut().set_tool(Tool::Draw(Kind::Text));
    let p = at(&h, 80.0, 80.0);
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(3);
    assert!(h.state().editing.is_some(), "a click with the text tool starts typing");
    h.event(egui::Event::Text("Hello".to_string()));
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    let ed = h.state();
    assert!(ed.editing.is_none());
    assert_eq!(ed.session.doc.objects.first().and_then(|o| o.shape.text()), Some("Hello"));
}

#[test]
fn crop_with_the_tool() {
    let mut h = harness(editor());
    h.run_steps(4);
    h.state_mut().set_tool(Tool::Crop);
    let (a, b) = (at(&h, 100.0, 50.0), at(&h, 300.0, 250.0));
    drag(&mut h, a, b);
    let c = h.state().crop.expect("a crop frame");
    assert!((c.w - 200.0).abs() < 3.0 && (c.h - 200.0).abs() < 3.0, "{c:?}");
    let ctx = h.ctx.clone();
    h.state_mut().apply_crop(&ctx);
    h.run_steps(2);
    let doc = &h.state().session.doc;
    assert!((i64::from(doc.width()) - 200).abs() <= 3 && (i64::from(doc.height()) - 200).abs() <= 3);
}

#[test]
fn select_move_and_delete() {
    let mut ed = editor();
    let id = ed
        .session
        .doc
        .add(
            Shape::Rect { rect: screengrabcraft_model::Rect::new(100.0, 100.0, 80.0, 60.0), radius: 0.0 },
            screengrabcraft_model::Style { fill: Color::RED, ..Kind::Rect.default_style() },
        )
        .unwrap();
    let mut h = harness(ed);
    h.run_steps(4);
    h.state_mut().set_tool(Tool::Select);
    let (a, b) = (at(&h, 140.0, 130.0), at(&h, 190.0, 160.0));
    drag(&mut h, a, b);
    let r = h.state().session.doc.get(id).and_then(|o| o.shape.rect()).unwrap();
    assert!((r.x - 150.0).abs() < 3.0 && (r.y - 130.0).abs() < 3.0, "moved: {r:?}");
    assert_eq!(h.state().session.selected(), vec![id]);
    h.key_press(egui::Key::Delete);
    h.run_steps(2);
    assert!(h.state().session.doc.objects.is_empty());
}

/// Drags out the area from (100, 100) to (300, 250).
fn drag_out_area(h: &mut Harness<'_, crate::overlay::Overlay>) {
    h.hover_at(pos2(100.0, 100.0));
    h.run_steps(1);
    h.drag_at(pos2(100.0, 100.0));
    h.run_steps(1);
    for i in 1..=5 {
        h.hover_at(pos2(100.0 + i as f32 * 40.0, 100.0 + i as f32 * 30.0));
        h.run_steps(1);
    }
    h.drop_at(pos2(300.0, 250.0));
    h.run_steps(2);
}

#[test]
fn the_overlay_chooses_an_area() {
    use crate::overlay::{Action, Outcome, Overlay, OverlayMonitor, Purpose};
    use screengrabcraft_model::IRect;
    let m = OverlayMonitor { rect: IRect::new(0, 0, 800, 600), picture: Raster::filled(800, 600, Color::WHITE).unwrap(), name: "Main".into() };
    let mut o = Overlay::new(vec![m], Vec::new(), Purpose::Picture);
    o.preview = false;
    let mut h = Harness::builder().with_size(egui::vec2(800.0, 600.0)).build_ui_state(
        |ui, o: &mut Overlay| {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| o.show_monitor(ui, 0));
        },
        o,
    );
    h.run_steps(2);
    drag_out_area(&mut h);
    match h.state().outcome() {
        Outcome::Chosen { area, action, .. } => {
            assert_eq!(*action, Action::Default);
            assert!((area.x - 100).abs() <= 1 && (area.y - 100).abs() <= 1 && (i64::from(area.w) - 200).abs() <= 2, "{area:?}");
        }
        other => panic!("{other:?}"),
    }
}

/// The overlay on a `w`×`h` screen with `area` chosen, and its bar showing.
fn overlay_with_bar(
    w: u32,
    h: u32,
    area: screengrabcraft_model::IRect,
    purpose: crate::overlay::Purpose,
) -> Harness<'static, crate::overlay::Overlay> {
    use crate::overlay::{Overlay, OverlayMonitor};
    use screengrabcraft_model::IRect;
    let m = OverlayMonitor { rect: IRect::new(0, 0, w, h), picture: Raster::filled(w, h, Color::WHITE).unwrap(), name: "Main".into() };
    let mut o = Overlay::new(vec![m], Vec::new(), purpose);
    o.preselect(0, area);
    let mut h = Harness::builder().with_size(egui::vec2(w as f32, h as f32)).build_ui_state(
        |ui, o: &mut Overlay| {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| o.show_monitor(ui, 0));
        },
        o,
    );
    h.run_steps(3);
    h
}

#[test]
fn a_recording_still_waits_to_be_started() {
    use crate::overlay::{Outcome, Overlay, OverlayMonitor, Purpose};
    use screengrabcraft_model::IRect;
    // Going straight to the default is for pictures; a recording would start before the owner is ready.
    for (purpose, label) in [(Purpose::Gif, "Record GIF"), (Purpose::Scrolling, "Start scrolling capture")] {
        let m = OverlayMonitor { rect: IRect::new(0, 0, 800, 600), picture: Raster::filled(800, 600, Color::WHITE).unwrap(), name: "Main".into() };
        let mut o = Overlay::new(vec![m], Vec::new(), purpose);
        o.preview = false;
        let mut h = Harness::builder().with_size(egui::vec2(800.0, 600.0)).build_ui_state(
            |ui, o: &mut Overlay| {
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| o.show_monitor(ui, 0));
            },
            o,
        );
        h.run_steps(2);
        drag_out_area(&mut h);
        assert_eq!(*h.state().outcome(), Outcome::Pending);
        h.get_by_label(label);
    }
}

#[test]
fn the_capture_bar_buttons_say_what_they_do_and_do_it() {
    use crate::overlay::{Action, Outcome, Purpose};
    use screengrabcraft_model::IRect;
    for (label, wanted) in [
        ("Copy & Edit", Some(Action::Default)),
        ("Edit", Some(Action::Edit)),
        ("Copy", Some(Action::Copy)),
        ("Save", Some(Action::Save)),
        ("Cancel", None),
    ] {
        let mut h = overlay_with_bar(800, 600, IRect::new(100, 100, 300, 200), Purpose::Picture);
        h.get_by_label(label).click();
        h.run_steps(3);
        match (h.state().outcome(), wanted) {
            (Outcome::Chosen { action, .. }, Some(a)) => assert_eq!(*action, a, "{label}"),
            (Outcome::Cancelled, None) => {}
            (other, _) => panic!("{label}: {other:?}"),
        }
    }
}

#[test]
fn the_capture_bar_stays_big_and_on_screen() {
    use crate::overlay::Purpose;
    use screengrabcraft_model::IRect;
    // Under the area, over the area's own bottom, and with the whole screen chosen.
    for area in [IRect::new(100, 100, 300, 200), IRect::new(500, 560, 280, 40), IRect::new(0, 0, 800, 600)] {
        let h = overlay_with_bar(800, 600, area, Purpose::Picture);
        for label in ["Copy & Edit", "Edit", "Copy", "Save", "Cancel"] {
            let r = h.get_by_label(label).rect();
            assert!(r.height() >= 48.0 && r.width() >= 100.0, "{label} {r:?}");
            assert!(r.min.x >= 0.0 && r.min.y >= 0.0 && r.max.x <= 800.0 && r.max.y <= 600.0, "{label} {area:?} {r:?}");
        }
    }
}

#[test]
fn the_default_has_a_button_only_when_no_other_does_the_same() {
    use crate::overlay::{Action, Purpose};
    use screengrabcraft_model::IRect;
    let mut h = overlay_with_bar(800, 600, IRect::new(100, 100, 300, 200), Purpose::Picture);
    h.state_mut().enter = Action::Copy;
    h.run_steps(2);
    assert!(h.query_by_label("Copy & Edit").is_none());
    h.get_by_label("Copy");
}

#[test]
fn a_screen_narrower_than_the_capture_bar_is_no_trouble() {
    use crate::overlay::{Outcome, Purpose};
    use screengrabcraft_model::IRect;
    // Placing the bar with `clamp` would panic here, losing the frozen screen.
    for purpose in [Purpose::Picture, Purpose::Gif, Purpose::Scrolling] {
        let h = overlay_with_bar(200, 90, IRect::new(10, 10, 100, 40), purpose);
        assert!(matches!(h.state().outcome(), Outcome::Pending));
    }
}

/// A host whose clipboard holds a picture.
struct PictureOnClipboard;

impl crate::services::Services for PictureOnClipboard {
    fn paste_image(&self) -> Option<Raster> {
        Raster::filled(40, 30, Color::RED).ok()
    }
}

#[test]
fn ctrl_v_pastes_a_picture_from_its_key_release() {
    // With a picture (and no text) on the clipboard, egui sends no Paste event, only the release
    // of V with the command key held.
    let mut ed = Editor::new(Box::new(PictureOnClipboard), EditorSettings::default());
    ed.session.replace(Document::new(Raster::filled(400, 300, Color::WHITE).unwrap()));
    let mut h = harness(ed);
    h.run_steps(3);
    h.event(egui::Event::Key { key: egui::Key::V, physical_key: None, pressed: false, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.run_steps(2);
    let doc = &h.state().session.doc;
    assert_eq!(doc.objects.len(), 1, "the picture was placed");
    assert_eq!(doc.objects.first().map(|o| o.shape.kind()), Some(Kind::Image));
    assert_eq!(doc.images.len(), 1);
}

/// The editor with an animated GIF open: three frames, a tenth of a second each.
fn with_a_gif() -> Harness<'static, Editor> {
    let mut w = screengrabcraft_render::anim::GifWriter::new(Vec::new(), 40, 30).unwrap();
    for (color, delay) in [(Color::RED, 0), (Color::rgb(0, 0, 255), 100), (Color::rgb(0, 255, 0), 100)] {
        w.push(Raster::filled(40, 30, color).unwrap(), delay).unwrap();
    }
    let bytes = w.finish(100).unwrap().0;
    let mut h = harness(editor());
    h.run_steps(3);
    let ctx = h.ctx.clone();
    h.state_mut().open_file(&ctx, OpenedFile { name: "Recording.gif".to_string(), path: None, bytes });
    h.run_steps(3);
    h
}

fn playing(h: &Harness<'_, Editor>) -> bool {
    h.state().playback.as_ref().is_some_and(|p| p.playing())
}

fn frame_up(h: &Harness<'_, Editor>) -> bool {
    h.state().playback.as_ref().is_some_and(|p| p.frame().is_some())
}

#[test]
fn a_gif_opens_as_its_first_frame_with_a_play_button() {
    let h = with_a_gif();
    assert_eq!((h.state().session.doc.width(), h.state().session.doc.height()), (40, 30));
    h.get_by_label("Play");
    assert!(!playing(&h) && !frame_up(&h), "nothing plays until asked");
}

#[test]
fn a_picture_has_no_play_button() {
    let mut h = harness(editor());
    h.run_steps(3);
    assert!(h.query_by_label("Play").is_none());
    let bytes = screengrabcraft_render::encode(&Raster::filled(40, 30, Color::RED).unwrap(), ImageFormat::Png, 90).unwrap();
    let ctx = h.ctx.clone();
    h.state_mut().open_file(&ctx, OpenedFile { name: "Capture.png".to_string(), path: None, bytes });
    h.run_steps(3);
    assert!(h.query_by_label("Play").is_none());
}

#[test]
fn play_runs_the_frames_and_pause_holds_one() {
    let mut h = with_a_gif();
    h.get_by_label("Play").click();
    h.run_steps(2);
    assert!(playing(&h) && frame_up(&h));
    let was = h.state().playback.as_ref().map(|p| p.elapsed_ms());
    h.run_steps(1);
    assert_ne!(h.state().playback.as_ref().map(|p| p.elapsed_ms()), was, "the frames move on");

    h.get_by_label("Pause").click();
    h.run_steps(2);
    assert!(!playing(&h));
    assert!(frame_up(&h), "the frame stays up");
    let held = h.state().playback.as_ref().map(|p| p.elapsed_ms());
    h.run_steps(3);
    assert_eq!(h.state().playback.as_ref().map(|p| p.elapsed_ms()), held);
    h.get_by_label("Play");
}

#[test]
fn a_gif_keeps_playing_round_and_round() {
    let mut h = with_a_gif();
    h.get_by_label("Play").click();
    h.run_steps(20);
    assert!(playing(&h), "it loops");
}

#[test]
fn a_change_to_the_picture_goes_back_to_it_from_the_frames() {
    let mut h = with_a_gif();
    h.get_by_label("Play").click();
    h.run_steps(2);
    assert!(playing(&h));
    h.state_mut().set_tool(Tool::Draw(Kind::Arrow));
    let (a, b) = (at(&h, 5.0, 5.0), at(&h, 35.0, 25.0));
    drag(&mut h, a, b);
    assert_eq!(h.state().session.doc.objects.len(), 1);
    assert!(!playing(&h) && !frame_up(&h), "the annotation is drawn on the picture, in view");
    h.get_by_label("Play");
}

#[test]
fn a_gif_cropped_to_something_else_stops_offering_to_play() {
    let mut h = with_a_gif();
    h.get_by_label("Play").click();
    h.run_steps(2);
    h.state_mut().session.execute("image.crop", &serde_json::json!({"x": 0, "y": 0, "w": 20, "h": 15})).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Play").is_none() && h.query_by_label("Pause").is_none());
    assert!(!frame_up(&h));
    assert!(h.state_mut().session.undo());
    h.run_steps(3);
    h.get_by_label("Play");
}

#[test]
fn opening_something_else_leaves_the_gif_behind() {
    let mut h = with_a_gif();
    h.state_mut().open_picture(Raster::filled(40, 30, Color::RED).unwrap(), None);
    h.run_steps(3);
    assert!(h.state().playback.is_none());
    assert!(h.query_by_label("Play").is_none());
}
