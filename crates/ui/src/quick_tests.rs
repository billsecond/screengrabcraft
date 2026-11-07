//! The editor's tool bar, Quick Styles, favourites and tool properties, driven headless the way a
//! person uses them.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use screengrabcraft_model::{CalloutForm, Color, Kind, Pt, Rect, Shape, Style};

use crate::editor::{Editor, Tool};
use crate::tests::{at, drag, editor, harness, ready};

/// Saves what the window looks like, when asked (`SGC_SHOTS=folder`).
fn shot(h: &mut Harness<'_, Editor>, name: &str) {
    if let Ok(dir) = std::env::var("SGC_SHOTS")
        && let Ok(img) = h.render()
    {
        let _ = img.save(format!("{dir}/{name}.png"));
    }
}

fn click(h: &mut Harness<'_, Editor>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

fn callout(ed: &mut Editor) -> screengrabcraft_model::ObjectId {
    let shape =
        Shape::Callout { rect: Rect::new(60.0, 40.0, 160.0, 60.0), tail: Pt::new(70.0, 160.0), text: "Hi".into(), form: CalloutForm::Rounded };
    ed.session.doc.add(shape, Kind::Callout.default_style()).unwrap()
}

#[test]
fn the_tool_bar_has_the_main_tools_in_the_middle_and_the_rest_under_more() {
    let mut h = harness(editor());
    h.run_steps(4);
    let labels = ["Favorites", "Arrow (A)", "Text (T)", "Callout (C)", "Shape (R, O)", "Stamp (G)", "Fill (F)", "Move (V)", "More tools"];
    let rects: Vec<egui::Rect> = labels.iter().map(|l| h.get_by_label(l).rect()).collect();
    for (pair, names) in rects.windows(2).zip(labels.windows(2)) {
        if let [a, b] = pair {
            assert!(a.max.x <= b.min.x && (a.center().y - b.center().y).abs() < 0.5, "{names:?} in order on one row");
        }
    }
    // In the middle of the bar (the window is 1200 wide, less the panel… the bar is the whole width).
    let (first, last) = (rects.first().unwrap(), rects.last().unwrap());
    let middle = (first.min.x + last.max.x) / 2.0;
    assert!((middle - 600.0).abs() < 40.0, "centred: {middle}");
    // The others are not on the bar.
    for gone in ["Line (L)", "Pen (P)", "Blur (B)", "Crop (K)", "Rectangle (R)", "Ellipse (O)"] {
        assert!(h.query_by_label(gone).is_none(), "{gone} is under More");
    }
    // The files at the left, copying and the zoom at the right, none of them over the tools.
    let save = h.get_by_label("Save (Ctrl+S)").rect();
    let copy = h.get_by_label("Copy the picture (Ctrl+C)").rect();
    assert!(save.max.x < first.min.x && last.max.x < copy.min.x, "{save:?} {first:?} {last:?} {copy:?}");
    shot(&mut h, "tool-bar");
}

#[test]
fn more_holds_the_other_tools_and_shows_the_one_picked() {
    let mut h = harness(editor());
    h.run_steps(4);
    click(&mut h, "More tools");
    for name in ["Line", "Pen", "Highlighter", "Step number", "Blur", "Pixelate", "Spotlight", "Magnify", "Crop"] {
        h.get_by_label(name);
    }
    shot(&mut h, "more-menu");
    click(&mut h, "Pen");
    assert_eq!(h.state().tool, Tool::Draw(Kind::Pen));
    assert!(h.query_by_label("Highlighter").is_none(), "the menu closed");
    h.get_by_label("More tools: Pen (P)");
    // Its key still picks it, and a main tool puts More back as it was.
    h.key_press(egui::Key::B);
    h.run_steps(2);
    h.get_by_label("More tools: Blur (B)");
    click(&mut h, "Arrow (A)");
    h.get_by_label("More tools");
}

#[test]
fn the_shape_button_draws_a_rectangle_or_an_ellipse() {
    let mut h = harness(editor());
    h.run_steps(4);
    click(&mut h, "Shape (R, O)");
    assert_eq!(h.state().tool, Tool::Draw(Kind::Rect));
    click(&mut h, "Shape options");
    click(&mut h, "Ellipse");
    assert_eq!(h.state().tool, Tool::Draw(Kind::Ellipse));
    let (a, b) = (at(&h, 60.0, 60.0), at(&h, 200.0, 160.0));
    drag(&mut h, a, b);
    assert_eq!(h.state().session.doc.objects.first().map(|o| o.shape.kind()), Some(Kind::Ellipse));
    // Another tool, then Shape again: the ellipse still.
    click(&mut h, "Arrow (A)");
    click(&mut h, "Shape (R, O)");
    assert_eq!(h.state().tool, Tool::Draw(Kind::Ellipse));
    // R and O pick each.
    h.key_press(egui::Key::R);
    h.run_steps(2);
    assert_eq!(h.state().tool, Tool::Draw(Kind::Rect));
    // A rounded rectangle from the picker.
    h.state_mut().session.select(Vec::new());
    h.run_steps(1);
    click(&mut h, "Shape options");
    click(&mut h, "Rounded rectangle");
    let (a, b) = (at(&h, 220.0, 60.0), at(&h, 360.0, 200.0));
    drag(&mut h, a, b);
    let last = h.state().session.doc.objects.last().map(|o| o.shape.clone());
    assert!(matches!(last, Some(Shape::Rect { radius, .. }) if radius > 0.0), "{last:?}");
}

#[test]
fn a_quick_style_restyles_the_selected_callout_in_one_undo_step() {
    let mut ed = editor();
    let id = callout(&mut ed);
    let mut h = ready(ed, Tool::Select);
    h.state_mut().session.select(vec![id]);
    h.run_steps(3);
    shot(&mut h, "callout-quick-styles");
    click(&mut h, "Blue, arrow");
    let o = h.state().session.doc.get(id).cloned().unwrap();
    let Shape::Callout { form, tail, rect, text } = o.shape else { panic!("a callout") };
    assert_eq!((form, text.as_str()), (CalloutForm::Arrow, "Hi"));
    assert!(tail.x > rect.right(), "the arrow points right: {tail:?} {rect:?}");
    assert_eq!(o.style.fill, Color::rgb(0x1E, 0x88, 0xE5));
    assert_eq!(o.style.font_size, Kind::Callout.default_style().font_size, "the text keeps its size");
    assert_eq!(h.state().session.undo_label(), Some("Apply Quick Style"));
    assert_eq!(h.state().tool, Tool::Select, "the tool stays the Move tool");
    // The callout tool keeps the look for the next one too.
    assert_eq!(h.state().tool_options(Kind::Callout).form, Some(CalloutForm::Arrow));
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.doc.get(id).map(|o| o.style), Some(Kind::Callout.default_style()));
}

#[test]
fn drawing_with_a_quick_style_draws_its_form_and_tail() {
    let mut h = ready(editor(), Tool::Draw(Kind::Callout));
    click(&mut h, "Green, tail down right");
    let (a, b) = (at(&h, 60.0, 40.0), at(&h, 200.0, 100.0));
    drag(&mut h, a, b);
    h.event(egui::Event::Text("Look".into()));
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let o = h.state().session.doc.objects.first().cloned();
    let Some(Shape::Callout { rect, tail, form, .. }) = o.as_ref().map(|o| o.shape.clone()) else { panic!("a callout: {o:?}") };
    assert_eq!(form, CalloutForm::Rounded);
    assert!(tail.x > rect.center().x && tail.y > rect.bottom(), "the tail goes down and right: {tail:?} {rect:?}");
    assert_eq!(o.map(|o| o.style.fill), Some(Color::rgb(0x43, 0xA0, 0x47)));
    // An arrow callout, with a click this time: its point is where the click was.
    click(&mut h, "Red, arrow");
    let p = at(&h, 300.0, 200.0);
    crate::tests::click_at(&mut h, p);
    h.event(egui::Event::Text("Here".into()));
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let o = h.state().session.doc.objects.last().cloned();
    let Some(Shape::Callout { rect, tail, form, .. }) = o.map(|o| o.shape) else { panic!("a second callout") };
    assert_eq!(form, CalloutForm::Arrow);
    assert!(tail.distance(Pt::new(300.0, 200.0)) < 3.0 && rect.right() < tail.x, "{tail:?} {rect:?}");
}

#[test]
fn starred_quick_styles_show_under_favorites() {
    let mut h = ready(editor(), Tool::Draw(Kind::Callout));
    assert!(h.state().settings.favorites.is_empty());
    click(&mut h, "Add Green, tail down right to Favorites");
    assert_eq!(h.state().settings.favorites.len(), 1);
    h.get_by_label("Remove Green, tail down right from Favorites");
    // Another tool, then Favorites: picking the favourite picks its tool and its look.
    click(&mut h, "Arrow (A)");
    click(&mut h, "Favorites");
    assert_eq!(h.state().tool, Tool::Favorites);
    shot(&mut h, "favorites");
    click(&mut h, "Callout favourite 1");
    assert_eq!(h.state().tool, Tool::Draw(Kind::Callout));
    assert_eq!(h.state().tool_style(Kind::Callout).fill, Color::rgb(0x43, 0xA0, 0x47));
    // Its star takes it away again.
    click(&mut h, "Favorites");
    click(&mut h, "Remove Callout favourite 1 from Favorites");
    assert!(h.state().settings.favorites.is_empty());
    h.get_by_label("Star a quick style (the star at its corner) to keep it here.");
}

#[test]
fn a_look_made_by_hand_can_be_kept_too() {
    let mut h = ready(editor(), Tool::Draw(Kind::Arrow));
    let style = Style { stroke: Color::rgb(1, 2, 3), width: 17.0, ..Kind::Arrow.default_style() };
    h.state_mut().settings.styles.insert(Kind::Arrow, style);
    h.run_steps(2);
    click(&mut h, "Add this look to Favorites");
    let f = h.state().settings.favorites.first().copied().unwrap();
    assert_eq!((f.kind, f.style.stroke, f.style.width), (Kind::Arrow, Color::rgb(1, 2, 3), 17.0));
    h.get_by_label("This look is in Favorites");
}

#[test]
fn the_theme_changes_the_quick_styles_colours() {
    let mut h = ready(editor(), Tool::Draw(Kind::Callout));
    h.get_by_label("Red, tail down left");
    h.state_mut().settings.quick_theme = "Soft".into();
    h.run_steps(2);
    assert!(h.query_by_label("Red, tail down left").is_none());
    click(&mut h, "Coral, tail down left");
    assert_eq!(h.state().tool_style(Kind::Callout).fill, Color::rgb(0xF2, 0x8B, 0x82));
}

#[test]
fn every_tool_has_quick_styles_in_its_panel() {
    for (tool, ..) in Tool::ALL {
        let mut h = ready(editor(), tool);
        let presets = match tool {
            Tool::Draw(k) => screengrabcraft_model::quick::presets(k, screengrabcraft_model::quick::theme("")),
            _ => continue,
        };
        for p in &presets {
            h.get_by_label(&p.name);
        }
        if let Tool::Draw(k) = tool {
            shot(&mut h, &format!("tool-{k:?}"));
        }
        let Some(last) = presets.last() else { continue };
        click(&mut h, &last.name);
        let Tool::Draw(k) = tool else { continue };
        assert!(last.look.matches(&crate::quick::tool_look(h.state(), k)), "{k:?}: {} is in use", last.name);
    }
}

#[test]
fn the_fill_tool_offers_its_colours_as_tiles() {
    let mut h = ready(editor(), Tool::Fill);
    click(&mut h, "Blue fill");
    assert_eq!(h.state().settings.fill.color, Color::rgb(0x1E, 0x88, 0xE5));
    click(&mut h, "None fill");
    assert_eq!(h.state().settings.fill.color, Color::TRANSPARENT);
    h.get_by_label("Tolerance");
}

#[test]
fn properties_fold_away_and_effects_are_a_click_away() {
    let mut h = ready(editor(), Tool::Draw(Kind::Arrow));
    for name in ["Line colour", "Shape options", "More options", "Thickness", "Opacity"] {
        h.get_by_label(name);
    }
    click(&mut h, "Properties");
    assert!(h.state().settings.hide_properties);
    assert!(h.query_by_label("Thickness").is_none(), "folded away");
    h.get_by_label("Red, arrow");
    click(&mut h, "Properties");
    h.get_by_label("Thickness");
    // Effects: the picture's effects in a menu.
    click(&mut h, "Picture effects");
    click(&mut h, "Invert Colors");
    assert_eq!(h.state().session.undo_label(), Some("Invert Colors"));
}

#[test]
fn the_tool_properties_change_the_colours_and_the_callout_form() {
    let mut ed = editor();
    let id = callout(&mut ed);
    let mut h = ready(ed, Tool::Select);
    h.state_mut().session.select(vec![id]);
    h.run_steps(3);
    for name in ["Fill colour", "Border colour", "Text colour", "Shape options", "More options", "Text size", "Border width", "Opacity"] {
        h.get_by_label(name);
    }
    // A colour from the fill's menu.
    click(&mut h, "Fill colour");
    click(&mut h, "#1E88E5");
    h.run_steps(3);
    assert_eq!(h.state().session.doc.get(id).map(|o| o.style.fill), Some(Color::rgb(0x1E, 0x88, 0xE5)));
    // Shadow, from More.
    let shadow = h.state().session.doc.get(id).map(|o| o.style.shadow);
    click(&mut h, "More options");
    click(&mut h, "Shadow");
    h.run_steps(3);
    assert_eq!(h.state().session.doc.get(id).map(|o| !o.style.shadow), shadow);
    // A cloud with its tail to the left, from the shape menu.
    click(&mut h, "Shape options");
    click(&mut h, "Cloud");
    // The menu stays open for the tail.
    click(&mut h, "Tail left");
    let o = h.state().session.doc.get(id).cloned().unwrap();
    let Shape::Callout { form, tail, rect, .. } = o.shape else { panic!("a callout") };
    assert_eq!(form, CalloutForm::Cloud);
    assert!(tail.x < rect.x && (tail.y - rect.center().y).abs() < 1.0, "{tail:?} {rect:?}");
}

#[test]
fn the_panel_fits_its_quick_styles_four_to_a_row() {
    let h = ready(editor(), Tool::Draw(Kind::Callout));
    let a = h.get_by_label("Red, tail down left").rect();
    let d = h.get_by_label("Yellow, tail down left").rect();
    let next = h.get_by_label("Red, tail down right").rect();
    assert!((a.center().y - d.center().y).abs() < 0.5, "one row");
    assert!(next.min.y > a.max.y, "the next look starts a row");
    let canvas = h.state().view.canvas;
    assert!(a.min.x > canvas.max.x && d.max.x <= 1200.0, "in the panel: {a:?} {d:?}");
    assert!(a.width() >= 40.0, "big enough to see: {a:?}");
}

#[test]
fn text_overlays_come_outlined_shadowed_on_boxes_and_as_titles() {
    let mut ed = editor();
    let style = Kind::Text.default_style();
    let id = ed.session.doc.add(Shape::Text { rect: Rect::new(40.0, 40.0, 260.0, 44.0), text: "Hello".into() }, style).unwrap();
    let mut h = ready(ed, Tool::Draw(Kind::Text));
    for name in ["Red, outlined text", "Red, shadowed text", "Red, label", "Red, see-through label", "Red, boxed text", "Red, title"] {
        h.get_by_label(name);
    }
    shot(&mut h, "text-quick-styles");
    // A title on the selected text: big, outlined, shadowed, in a taller box; one undo step.
    h.state_mut().session.select(vec![id]);
    h.run_steps(2);
    click(&mut h, "Blue, title");
    let o = h.state().session.doc.get(id).cloned().unwrap();
    assert_eq!(o.style.font_size, screengrabcraft_model::quick::TITLE_SIZE);
    assert!(o.style.has_text_outline() && o.style.shadow);
    assert!(o.shape.rect().unwrap().h > 44.0, "the box grew for the title");
    assert_eq!(h.state().session.undo_label(), Some("Apply Quick Style"));
    // An outline from the tool properties, with its width.
    click(&mut h, "Red, text");
    assert_eq!(h.state().session.doc.get(id).map(|o| o.style.font_size), Some(screengrabcraft_model::quick::TITLE_SIZE), "plain looks keep the size");
    click(&mut h, "Text outline colour");
    click(&mut h, "#111111");
    h.run_steps(3);
    let o = h.state().session.doc.get(id).cloned().unwrap();
    assert_eq!(o.style.text_outline, Color::rgb(0x11, 0x11, 0x11));
    h.get_by_label("Text outline");
    // Drawing with the title look: the next text box is a title too.
    h.state_mut().session.select(Vec::new());
    h.run_steps(2);
    click(&mut h, "Green, title");
    assert_eq!(h.state().tool_style(Kind::Text).font_size, screengrabcraft_model::quick::TITLE_SIZE);
    let (a, b) = (at(&h, 40.0, 150.0), at(&h, 360.0, 200.0));
    drag(&mut h, a, b);
    h.event(egui::Event::Text("Big".into()));
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let last = h.state().session.doc.objects.last().cloned().unwrap();
    assert_eq!((last.shape.text(), last.style.font_size), (Some("Big"), screengrabcraft_model::quick::TITLE_SIZE));
}
