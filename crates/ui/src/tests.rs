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

// ---- staying on the picture ----

/// Every drawing tool.
fn draw_tools() -> Vec<(Tool, Kind)> {
    Tool::BAR.into_iter().filter_map(|(tool, _, _, _)| if let Tool::Draw(kind) = tool { Some((tool, kind)) } else { None }).collect()
}

fn ready(ed: Editor, tool: Tool) -> Harness<'static, Editor> {
    let mut h = harness(ed);
    h.run_steps(4);
    h.state_mut().set_tool(tool);
    h.run_steps(1);
    h
}

fn click_at(h: &mut Harness<'_, Editor>, p: Pos2) {
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(3);
}

fn filled_rect(ed: &mut Editor, x: f32, y: f32, w: f32, h: f32) -> screengrabcraft_model::ObjectId {
    let style = screengrabcraft_model::Style { fill: Color::RED, ..Kind::Rect.default_style() };
    ed.session.doc.add(Shape::Rect { rect: screengrabcraft_model::Rect::new(x, y, w, h), radius: 0.0 }, style).unwrap()
}

/// The picture with a hair of room for rounding.
fn picture(ed: &Editor) -> screengrabcraft_model::Rect {
    ed.session.doc.bounds().inflate(0.01)
}

#[test]
fn a_drag_from_outside_the_picture_draws_nothing() {
    for (tool, kind) in draw_tools() {
        let mut h = ready(editor(), tool);
        let (a, b) = (at(&h, -40.0, 100.0), at(&h, 200.0, 160.0));
        drag(&mut h, a, b);
        assert!(h.state().session.doc.objects.is_empty(), "{kind:?} drew from outside");
        assert!(h.state().editing.is_none(), "{kind:?} started typing");
        assert!(h.state().session.undo_label().is_none(), "{kind:?} left an undo step");
    }
}

#[test]
fn a_click_outside_the_picture_places_nothing() {
    for (tool, kind) in draw_tools() {
        let mut h = ready(editor(), tool);
        let p = at(&h, 200.0, -30.0);
        click_at(&mut h, p);
        assert!(h.state().session.doc.objects.is_empty(), "{kind:?} placed something outside");
        assert!(h.state().editing.is_none(), "{kind:?} started typing");
    }
}

#[test]
fn a_click_outside_the_picture_stops_typing_and_clears_the_selection() {
    let mut h = ready(editor(), Tool::Draw(Kind::Text));
    let p = at(&h, 80.0, 80.0);
    click_at(&mut h, p);
    h.event(egui::Event::Text("Hello".to_string()));
    h.run_steps(3);
    let out = at(&h, -30.0, 80.0);
    click_at(&mut h, out);
    assert!(h.state().editing.is_none(), "typing stopped");
    assert_eq!(h.state().session.doc.objects.len(), 1, "the text is kept and nothing new was placed");
    click_at(&mut h, out);
    assert!(h.state().session.selected().is_empty(), "the selection is cleared");
    assert_eq!(h.state().session.doc.objects.len(), 1);
}

#[test]
fn drawing_stops_at_the_edge_of_the_picture() {
    for (tool, kind) in draw_tools() {
        for (x, y) in [(480.0, 390.0), (-80.0, -90.0), (480.0, 180.0), (230.0, 390.0)] {
            let mut h = ready(editor(), tool);
            let (a, b) = (at(&h, 200.0, 150.0), at(&h, x, y));
            drag(&mut h, a, b);
            let ed = h.state();
            assert_eq!(ed.session.doc.objects.len(), 1, "{kind:?} to {x},{y}");
            for o in &ed.session.doc.objects {
                assert!(picture(ed).encloses(o.shape.extent()), "{kind:?} to {x},{y}: {:?}", o.shape);
            }
        }
    }
}

#[test]
fn a_pen_stroke_runs_along_the_edge() {
    let mut h = ready(editor(), Tool::Draw(Kind::Pen));
    let (a, b) = (at(&h, 300.0, 100.0), at(&h, 520.0, 200.0));
    drag(&mut h, a, b);
    let Some(Shape::Pen { points }) = h.state().session.doc.objects.first().map(|o| o.shape.clone()) else { panic!("no pen stroke") };
    let last = points.last().unwrap();
    assert!((last.x - 400.0).abs() < 0.01, "on the edge: {last:?}");
    assert!((last.y - 200.0).abs() < 3.0, "and still following the pointer: {last:?}");
}

#[test]
fn a_step_badge_stays_whole_at_the_edge() {
    let mut h = ready(editor(), Tool::Draw(Kind::Step));
    let (a, b) = (at(&h, 300.0, 100.0), at(&h, 520.0, 100.0));
    drag(&mut h, a, b);
    let ed = h.state();
    let o = ed.session.doc.objects.first().unwrap();
    assert!(picture(ed).encloses(o.shape.bounds(&o.style)), "{:?}", o.shape.bounds(&o.style));
}

/// A drag with Shift held.
fn shift_drag(h: &mut Harness<'_, Editor>, from: Pos2, to: Pos2) {
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
    h.run_steps(1);
    drag(h, from, to);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run_steps(1);
}

#[test]
fn a_shift_square_stops_at_the_edge_and_stays_square() {
    let mut h = ready(editor(), Tool::Draw(Kind::Rect));
    let (a, b) = (at(&h, 200.0, 150.0), at(&h, 480.0, 200.0));
    shift_drag(&mut h, a, b);
    let ed = h.state();
    let r = ed.session.doc.objects.first().and_then(|o| o.shape.rect()).unwrap();
    assert!(picture(ed).encloses(r), "{r:?}");
    assert!((r.w - r.h).abs() < 0.1, "square: {r:?}");
    assert!(r.w > 100.0, "as big as it fits: {r:?}");
}

#[test]
fn a_snapped_line_stops_at_the_edge_on_its_angle() {
    let mut h = ready(editor(), Tool::Draw(Kind::Line));
    let (a, b) = (at(&h, 200.0, 150.0), at(&h, 470.0, 400.0));
    shift_drag(&mut h, a, b);
    let ed = h.state();
    let Some(Shape::Line { from, to }) = ed.session.doc.objects.first().map(|o| o.shape.clone()) else { panic!("no line") };
    assert!(picture(ed).contains(to), "{to:?}");
    assert!(((to.x - from.x).abs() - (to.y - from.y).abs()).abs() < 0.5, "still at 45 degrees: {from:?} to {to:?}");
}

#[test]
fn a_move_stops_at_the_edge_and_waits_for_the_pointer() {
    let mut ed = editor();
    let id = filled_rect(&mut ed, 100.0, 100.0, 80.0, 60.0);
    let mut h = ready(ed, Tool::Select);
    let a = at(&h, 140.0, 130.0);
    h.hover_at(a);
    h.run_steps(2);
    h.drag_at(a);
    h.run_steps(2);
    for x in [200.0, 300.0, 400.0, 500.0, 620.0] {
        let p = at(&h, x, 130.0);
        h.hover_at(p);
        h.run_steps(1);
    }
    let bounds = |h: &Harness<'_, Editor>| h.state().session.doc.get(id).map(|o| o.shape.bounds(&o.style)).unwrap();
    let stopped = bounds(&h);
    assert!(picture(h.state()).encloses(stopped), "{stopped:?}");
    assert!(stopped.right() > 399.0, "right up to the edge: {stopped:?}");
    // Coming back, it doesn't move until the pointer is back where it took hold.
    let p = at(&h, 500.0, 130.0);
    h.hover_at(p);
    h.run_steps(1);
    assert_eq!(bounds(&h), stopped);
    let p = at(&h, 240.0, 130.0);
    h.hover_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(3);
    let r = h.state().session.doc.get(id).and_then(|o| o.shape.rect()).unwrap();
    assert!((r.x - 200.0).abs() < 3.0, "under the pointer again: {r:?}");
    assert_eq!(h.state().session.undo_label(), Some("Move"));
}

#[test]
fn an_annotation_over_the_edge_moves_in_without_jumping() {
    let mut ed = editor();
    let id = filled_rect(&mut ed, -50.0, 100.0, 80.0, 60.0);
    let mut h = ready(ed, Tool::Select);
    let (a, b) = (at(&h, 10.0, 130.0), at(&h, 20.0, 130.0));
    drag(&mut h, a, b);
    let r = h.state().session.doc.get(id).and_then(|o| o.shape.rect()).unwrap();
    assert!((r.x - -40.0).abs() < 3.0, "moved by what the pointer moved: {r:?}");
    let (a, b) = (at(&h, 10.0, 130.0), at(&h, -60.0, 130.0));
    drag(&mut h, a, b);
    let r = h.state().session.doc.get(id).and_then(|o| o.shape.rect()).unwrap();
    assert!(r.x >= -40.0 - 3.0, "no further out: {r:?}");
}

#[test]
fn an_annotation_over_the_edge_is_not_picked_up_from_outside() {
    let mut ed = editor();
    let id = filled_rect(&mut ed, -50.0, 100.0, 80.0, 60.0);
    let mut h = ready(ed, Tool::Select);
    let (a, b) = (at(&h, -20.0, 130.0), at(&h, 100.0, 130.0));
    drag(&mut h, a, b);
    let r = h.state().session.doc.get(id).and_then(|o| o.shape.rect()).unwrap();
    assert_eq!(r.x, -50.0, "{r:?}");
}

#[test]
fn a_handle_stops_at_the_edge() {
    let mut ed = editor();
    let id = filled_rect(&mut ed, 100.0, 100.0, 80.0, 60.0);
    ed.session.select(vec![id]);
    let mut h = ready(ed, Tool::Select);
    let (a, b) = (at(&h, 180.0, 160.0), at(&h, 520.0, 420.0));
    drag(&mut h, a, b);
    let r = h.state().session.doc.get(id).and_then(|o| o.shape.rect()).unwrap();
    assert!((r.right() - 400.0).abs() < 0.01 && (r.bottom() - 300.0).abs() < 0.01, "{r:?}");
    assert_eq!(h.state().session.undo_label(), Some("Resize"));
}

#[test]
fn a_square_handle_stops_short_of_the_edge_rather_than_going_over() {
    let mut ed = editor();
    let style = Kind::Stamp.default_style();
    let stamp = screengrabcraft_model::StampKind::default();
    let rect = screengrabcraft_model::Rect::new(300.0, 100.0, 60.0, 60.0);
    let id = ed.session.doc.add(Shape::Stamp { rect, stamp }, style).unwrap();
    ed.session.select(vec![id]);
    let mut h = ready(ed, Tool::Select);
    let (a, b) = (at(&h, 360.0, 160.0), at(&h, 500.0, 170.0));
    drag(&mut h, a, b);
    let ed = h.state();
    let r = ed.session.doc.get(id).and_then(|o| o.shape.rect()).unwrap();
    assert!(picture(ed).encloses(r), "{r:?}");
}

#[test]
fn a_handle_on_the_edge_can_be_taken_from_just_outside() {
    let mut ed = editor();
    let id = filled_rect(&mut ed, 300.0, 100.0, 100.0, 60.0);
    ed.session.select(vec![id]);
    let mut h = ready(ed, Tool::Select);
    let scale = h.state().view.scale();
    let (a, b) = (at(&h, 400.0 + 4.0 / scale, 130.0), at(&h, 350.0, 130.0));
    drag(&mut h, a, b);
    let r = h.state().session.doc.get(id).and_then(|o| o.shape.rect()).unwrap();
    assert!((r.w - 50.0).abs() < 8.0, "{r:?}");
}

#[test]
fn a_rubber_band_starts_on_the_picture_and_stays_on_it() {
    let mut ed = editor();
    let id = filled_rect(&mut ed, 300.0, 200.0, 40.0, 40.0);
    let mut h = ready(ed, Tool::Select);
    // From outside: nothing.
    let (a, b) = (at(&h, 250.0, -30.0), at(&h, 390.0, 290.0));
    h.hover_at(a);
    h.run_steps(2);
    h.drag_at(a);
    h.run_steps(2);
    h.hover_at(b);
    h.run_steps(2);
    assert!(h.state().drag.is_none(), "no band from outside");
    h.drop_at(b);
    h.run_steps(3);
    assert!(h.state().session.selected().is_empty());
    // From inside: as far as the edge, and it still selects.
    let (a, b) = (at(&h, 250.0, 150.0), at(&h, 500.0, 420.0));
    h.hover_at(a);
    h.run_steps(2);
    h.drag_at(a);
    h.run_steps(2);
    h.hover_at(b);
    h.run_steps(2);
    let Some(crate::canvas::Drag::Rubber { now, .. }) = h.state().drag.clone() else { panic!("no rubber band") };
    assert!(picture(h.state()).contains(now), "{now:?}");
    h.drop_at(b);
    h.run_steps(3);
    assert_eq!(h.state().session.selected(), vec![id]);
}

#[test]
fn a_crop_starts_on_the_picture_or_on_a_handle() {
    let mut h = ready(editor(), Tool::Crop);
    let whole = h.state().crop;
    let (a, b) = (at(&h, 200.0, -40.0), at(&h, 300.0, 200.0));
    drag(&mut h, a, b);
    assert_eq!(h.state().crop, whole, "a drag from outside leaves the frame alone");
    // The frame's corner sits on the picture's: it can be taken from just outside.
    let scale = h.state().view.scale();
    let (a, b) = (at(&h, -3.0 / scale, -3.0 / scale), at(&h, 100.0, 50.0));
    drag(&mut h, a, b);
    let c = h.state().crop.expect("a crop frame");
    assert!((c.x - 100.0).abs() < 8.0 && (c.y - 50.0).abs() < 8.0, "{c:?}");
    let (a, b) = (at(&h, 400.0, 300.0), at(&h, 600.0, 500.0));
    drag(&mut h, a, b);
    let c = h.state().crop.expect("a crop frame");
    assert!(picture(h.state()).encloses(c), "{c:?}");
}

#[test]
fn what_a_click_places_near_an_edge_is_all_on_the_picture() {
    for kind in [Kind::Text, Kind::Callout, Kind::Step, Kind::Stamp, Kind::Magnify] {
        for (x, y) in [(398.0, 298.0), (2.0, 2.0), (398.0, 2.0), (2.0, 298.0)] {
            let mut h = ready(editor(), Tool::Draw(kind));
            let p = at(&h, x, y);
            click_at(&mut h, p);
            let ed = h.state();
            assert_eq!(ed.session.doc.objects.len(), 1, "{kind:?} at {x},{y}");
            let o = ed.session.doc.objects.first().unwrap();
            assert!(picture(ed).encloses(o.shape.bounds(&o.style)), "{kind:?} at {x},{y}: {:?}", o.shape.bounds(&o.style));
        }
    }
}

#[test]
fn what_a_click_places_fits_on_a_tiny_picture() {
    for kind in [Kind::Callout, Kind::Stamp, Kind::Magnify] {
        let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
        ed.session.replace(Document::new(Raster::filled(60, 40, Color::WHITE).unwrap()));
        ed.view.fit_pending = true;
        let mut h = ready(ed, Tool::Draw(kind));
        let p = at(&h, 30.0, 20.0);
        click_at(&mut h, p);
        let ed = h.state();
        let o = ed.session.doc.objects.first().unwrap();
        assert!(ed.session.doc.bounds().inflate(o.style.width).encloses(o.shape.extent()), "{kind:?}: {:?}", o.shape.extent());
    }
}

#[test]
fn a_text_box_growing_past_the_bottom_moves_up() {
    let mut h = ready(editor(), Tool::Draw(Kind::Text));
    let p = at(&h, 100.0, 280.0);
    click_at(&mut h, p);
    assert!(h.state().editing.is_some());
    h.event(egui::Event::Text("one\ntwo\nthree\nfour\nfive".to_string()));
    h.run_steps(3);
    let ed = h.state();
    let o = ed.session.doc.objects.first().unwrap();
    let r = o.shape.rect().unwrap();
    assert!(r.h > 60.0, "it grew: {r:?}");
    assert!(picture(ed).encloses(o.shape.bounds(&o.style)), "{r:?}");
}

#[test]
fn pasted_text_is_all_on_the_picture() {
    let mut h = ready(editor(), Tool::Select);
    h.state_mut().view.center = screengrabcraft_model::Pt::new(200.0, 295.0);
    h.event(egui::Event::Paste("one\ntwo\nthree\nfour\nfive\nsix".to_string()));
    h.run_steps(3);
    let ed = h.state();
    let o = ed.session.doc.objects.first().expect("the pasted text");
    assert!(picture(ed).encloses(o.shape.bounds(&o.style)), "{:?}", o.shape.bounds(&o.style));
}

#[test]
fn the_arrow_keys_stop_at_the_edge() {
    let mut ed = editor();
    let id = filled_rect(&mut ed, 330.0, 100.0, 60.0, 60.0);
    ed.session.select(vec![id]);
    let mut h = ready(ed, Tool::Select);
    for _ in 0..30 {
        h.key_press(egui::Key::ArrowRight);
        h.run_steps(1);
    }
    let ed = h.state();
    let o = ed.session.doc.get(id).unwrap();
    assert!(picture(ed).encloses(o.shape.bounds(&o.style)), "{:?}", o.shape.bounds(&o.style));
}

#[test]
fn a_thicker_line_or_bigger_badge_stays_on_the_picture() {
    let mut ed = editor();
    let style = Kind::Step.default_style();
    let center = screengrabcraft_model::Pt::new(385.0, 150.0);
    let id = ed.session.doc.add(Shape::Step { center, number: 1 }, style).unwrap();
    ed.session.doc.bring_inside(id);
    ed.session.select(vec![id]);
    let bigger = screengrabcraft_model::Style { font_size: 60.0, ..style };
    crate::props::commit_style(&mut ed, Kind::Step, &style, &bigger, true);
    let o = ed.session.doc.get(id).unwrap();
    assert_eq!(o.style.font_size, 60.0);
    assert!(picture(&ed).encloses(o.shape.bounds(&o.style)), "{:?}", o.shape.bounds(&o.style));
}

#[test]
fn a_new_colour_leaves_an_annotation_over_the_edge_where_it_is() {
    let mut ed = editor();
    let id = filled_rect(&mut ed, 380.0, 100.0, 80.0, 60.0);
    ed.session.select(vec![id]);
    let style = ed.session.doc.get(id).unwrap().style;
    let blue = screengrabcraft_model::Style { stroke: Color::rgba(0, 0, 255, 255), ..style };
    crate::props::commit_style(&mut ed, Kind::Rect, &style, &blue, true);
    assert_eq!(ed.session.doc.get(id).and_then(|o| o.shape.rect()).map(|r| r.x), Some(380.0));
}

#[test]
fn the_cursor_says_nothing_can_be_drawn_outside_the_picture() {
    let mut h = ready(editor(), Tool::Draw(Kind::Rect));
    let p = at(&h, 200.0, 150.0);
    h.hover_at(p);
    h.run_steps(2);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Crosshair);
    let p = at(&h, -30.0, 150.0);
    h.hover_at(p);
    h.run_steps(2);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Default);
}

#[test]
fn the_view_still_pans_from_outside_the_picture() {
    let mut h = ready(editor(), Tool::Draw(Kind::Rect));
    let before = h.state().view.center;
    let (a, b) = (at(&h, -30.0, 150.0), at(&h, 20.0, 180.0));
    h.hover_at(a);
    h.run_steps(2);
    let middle = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Middle, pressed, modifiers: egui::Modifiers::NONE };
    h.event(middle(a, true));
    h.run_steps(2);
    h.hover_at(b);
    h.run_steps(2);
    h.event(middle(b, false));
    h.run_steps(2);
    assert!(h.state().view.center.distance(before) > 10.0, "{:?}", h.state().view.center);
    assert!(h.state().session.doc.objects.is_empty());
}
