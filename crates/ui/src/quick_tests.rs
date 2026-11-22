//! The editor's tool bar, Quick Styles, favourites and tool properties, driven headless the way a
//! person uses them.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use screengrabcraft_model::{CalloutForm, Color, Kind, Pt, Rect, Shape, Style};

use crate::editor::{Editor, Tool};
use crate::tests::{at, drag, editor, harness, ready, ready_tall};

/// Saves what the window looks like, when asked (`SGC_SHOTS=folder`).
pub(crate) fn shot(h: &mut Harness<'_, Editor>, name: &str) {
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
    let shape = Shape::Callout {
        rect: Rect::new(60.0, 40.0, 160.0, 60.0),
        tail: Pt::new(70.0, 160.0),
        text: "Hi".into(),
        form: CalloutForm::Rounded,
        tails: Vec::new(),
    };
    ed.session.doc.add(shape, Kind::Callout.default_style()).unwrap()
}

/// Every tool's button on the tool bar, in order.
const BAR: [&str; 19] = [
    "Favorites",
    "Move (V)",
    "Hand (D)",
    "Selection (Q)",
    "Crop (K)",
    "Arrow (A)",
    "Line (L)",
    "Text (T)",
    "Callout (C)",
    "Shape (R, O)",
    "Stamp (G)",
    "Step number (N)",
    "Pen (P)",
    "Highlighter (H)",
    "Fill (F)",
    "Blur (B)",
    "Pixelate (X)",
    "Spotlight (S)",
    "Magnify (M)",
];

fn sized(width: f32) -> Harness<'static, Editor> {
    let mut h = Harness::builder().with_size(egui::vec2(width, 800.0)).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        editor(),
    );
    h.run_steps(4);
    h
}

#[test]
fn every_tool_is_on_the_tool_bar_at_full_size_in_the_middle() {
    let mut h = sized(1600.0);
    let rects: Vec<egui::Rect> = BAR.iter().map(|l| h.get_by_label(l).rect()).collect();
    for (pair, names) in rects.windows(2).zip(BAR.windows(2)) {
        if let [a, b] = pair {
            assert!(a.max.x <= b.min.x && (a.center().y - b.center().y).abs() < 0.5, "{names:?} in order on one row");
        }
    }
    for (r, name) in rects.iter().zip(BAR) {
        assert!((r.width() - crate::editor::BAR_BUTTON).abs() < 0.5, "{name} full size: {r:?}");
    }
    let (first, last) = (rects.first().unwrap(), rects.last().unwrap());
    let middle = (first.min.x + last.max.x) / 2.0;
    assert!((middle - 800.0).abs() < 60.0, "centred: {middle}");
    // The files at the left, copying and the zoom at the right, none of them over the tools.
    let save = h.get_by_label("Delete the selection (Del)").rect();
    let copy = h.get_by_label("Copy the picture (Ctrl+C)").rect();
    assert!(save.max.x < first.min.x && last.max.x < copy.min.x, "{save:?} {first:?} {last:?} {copy:?}");
    assert!(h.query_by_label("More tools").is_none(), "nothing is tucked away");
    shot(&mut h, "tool-bar");
}

#[test]
fn a_narrow_window_puts_the_tools_on_a_row_of_their_own_not_smaller() {
    for width in [800.0, 1000.0, 1200.0, 1400.0] {
        let h = sized(width);
        let delete = h.get_by_label("Delete the selection (Del)").rect();
        let zoom_out = h.get_by_label("Zoom out (Ctrl+-)").rect();
        let canvas_top = h.state().view.to_screen(screengrabcraft_model::Pt::new(0.0, 0.0)).y;
        for name in BAR {
            let r = h.get_by_label(name).rect();
            assert!((r.width() - crate::editor::BAR_BUTTON).abs() < 0.5, "at {width}: {name} full size: {r:?}");
            assert!(!r.intersects(delete) && !r.intersects(zoom_out), "at {width}: {name} {r:?} over {delete:?} or {zoom_out:?}");
            assert!(r.max.x <= width && r.min.x >= 0.0, "at {width}: {name} in the window: {r:?}");
            assert!(r.max.y <= canvas_top, "at {width}: {name} above the picture: {r:?} (picture at {canvas_top})");
        }
    }
}

#[test]
fn every_tool_button_picks_its_tool_and_shows_it_picked() {
    let mut h = sized(1600.0);
    for (name, tool) in [
        ("Pen (P)", Tool::Draw(Kind::Pen)),
        ("Blur (B)", Tool::Draw(Kind::Blur)),
        ("Crop (K)", Tool::Crop),
        ("Callout (C)", Tool::Draw(Kind::Callout)),
    ] {
        click(&mut h, name);
        assert_eq!(h.state().tool, tool, "{name}");
    }
    h.key_press(egui::Key::H);
    h.run_steps(2);
    assert_eq!(h.state().tool, Tool::Draw(Kind::Highlight));
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

/// The Step tool's Shape options: steps in capital letters, then a pin; the badges count on in
/// letters beside the numbered one; a quick style gives a selected badge another outline and
/// keeps its letter.
#[test]
fn steps_count_in_letters_on_badges_of_other_shapes() {
    use screengrabcraft_model::{BadgeShape, StepLabels};
    let mut h = ready_tall(editor(), Tool::Draw(Kind::Step));
    let label = |h: &Harness<'_, Editor>, i: usize| h.state().session.doc.objects.get(i).and_then(|o| o.shape.step_label());
    let p = at(&h, 40.0, 40.0);
    crate::tests::click_at(&mut h, p);
    assert_eq!(label(&h, 0).as_deref(), Some("1"));
    h.state_mut().session.select(Vec::new());
    h.run_steps(2);
    click(&mut h, "Shape options");
    shot(&mut h, "step-shape-options");
    click(&mut h, "A, B, C");
    assert_eq!(h.state().tool_options(Kind::Step).labels, Some(StepLabels::Upper));
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.get_by_label_contains("The next step is A.");
    let p = at(&h, 100.0, 40.0);
    crate::tests::click_at(&mut h, p);
    h.state_mut().session.select(Vec::new());
    h.run_steps(2);
    click(&mut h, "Shape options");
    click(&mut h, "Pin");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let p = at(&h, 160.0, 40.0);
    crate::tests::click_at(&mut h, p);
    assert_eq!((label(&h, 1).as_deref(), label(&h, 2).as_deref()), (Some("A"), Some("B")), "letters count on their own");
    let badge = |h: &Harness<'_, Editor>, i: usize| match h.state().session.doc.objects.get(i).map(|o| &o.shape) {
        Some(Shape::Step { badge, .. }) => Some(*badge),
        _ => None,
    };
    assert_eq!((badge(&h, 0), badge(&h, 1), badge(&h, 2)), (Some(BadgeShape::Circle), Some(BadgeShape::Circle), Some(BadgeShape::Pin)));
    // The letter badge selected, a square look: square, and still A.
    let id = h.state().session.doc.objects.get(1).map(|o| o.id).unwrap();
    h.state_mut().set_tool(Tool::Select);
    h.state_mut().session.select(vec![id]);
    h.run_steps(3);
    click(&mut h, "Green, square badge");
    assert_eq!((badge(&h, 1), label(&h, 1).as_deref()), (Some(BadgeShape::Rounded), Some("A")));
    assert_eq!(h.state().session.undo_label(), Some("Apply Quick Style"));
    h.get_by_label_contains("shows A");
    // Every badge and way of counting, for the picture of it.
    for (i, badge) in BadgeShape::ALL.into_iter().enumerate() {
        for (j, labels) in StepLabels::ALL.into_iter().enumerate() {
            let center = Pt::new(40.0 + i as f32 * 60.0, 110.0 + j as f32 * 36.0);
            let style = Kind::Step.default_style();
            h.state_mut().session.doc.add(Shape::Step { center, number: [1, 2, 4, 9, 38][j], labels, badge }, style).unwrap();
        }
    }
    h.state_mut().session.touch();
    h.run_steps(3);
    shot(&mut h, "step-badges");
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
    let Shape::Callout { form, tail, rect, text, .. } = o.shape else { panic!("a callout") };
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
        let mut h = ready_tall(editor(), tool);
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

/// Properties are always shown (there's no button to fold them away), with the Effects pane
/// docked under them: each effect a section with its settings and Apply.
#[test]
fn properties_stay_shown_over_the_effects_pane() {
    // A window as tall as a full-HD screen's, so an opened effect is in sight in the pane.
    let mut h = crate::tests::harness_sized(editor(), egui::vec2(1200.0, 1080.0));
    h.run_steps(4);
    h.state_mut().set_tool(Tool::Draw(Kind::Arrow));
    h.run_steps(2);
    for name in ["Red, arrow", "Line colour", "Shape options", "More options", "Thickness", "Opacity"] {
        h.get_by_label(name);
    }
    assert!(h.query_by_label("Properties").is_none(), "no button to fold them away");
    assert!(h.query_by_label("Picture effects").is_none(), "the effects aren't a menu under a button");
    // The effects are listed, closed; one opened shows its settings and Apply.
    for name in ["Border", "Drop Shadow", "Torn Edge", "Round Corners", "Brightness and Contrast", crate::effects::COLOURS] {
        h.get_by_label(name);
    }
    assert!(h.query_by_label("Apply Border").is_none());
    click(&mut h, "Border");
    shot(&mut h, "effects-pane");
    click(&mut h, "Apply Border");
    assert_eq!(h.state().session.undo_label(), Some("Border"));
    assert_eq!((h.state().session.doc.width(), h.state().session.doc.height()), (408, 308), "four pixels round");
    // Closed again, and the colours opened: Invert is one click.
    click(&mut h, "Border");
    click(&mut h, crate::effects::COLOURS);
    // The section opens with a short slide: once it has, Invert is where it stays.
    h.run_steps(20);
    click(&mut h, "Invert Colors");
    assert_eq!(h.state().session.undo_label(), Some("Invert Colors"));
}

/// The panes' left edge is dragged to make them wider or narrower, and the bar between them to
/// give Properties or Effects more of the height; both are kept in bounds and remembered.
#[test]
fn the_panes_are_resized_by_their_handles() {
    use crate::props::{EDGE_NAME, PANEL_WIDTH, SPLIT_NAME, panes_width};
    let mut h = ready(editor(), Tool::Draw(Kind::Arrow));
    let window = 1200.0;
    assert_eq!(panes_width(h.state(), window), PANEL_WIDTH);
    let thickness_before = h.get_by_label("Thickness").rect();
    // Wider, by the edge.
    let edge = h.get_by_label(EDGE_NAME).rect().center();
    drag(&mut h, edge, egui::pos2(edge.x - 80.0, edge.y));
    let wide = h.state().settings.side_width.unwrap_or_default();
    assert!((wide - (PANEL_WIDTH + 80.0)).abs() < 2.0, "{wide}");
    let edge_now = h.get_by_label(EDGE_NAME).rect().center();
    assert!((edge_now.x - (edge.x - 80.0)).abs() < 2.0, "the edge follows: {edge_now:?}");
    assert!(h.get_by_label("Thickness").rect().left() < thickness_before.left() - 60.0, "what's in them moves with it");
    // As wide as the window leaves room for, and no narrower than they work at.
    drag(&mut h, edge_now, egui::pos2(10.0, edge.y));
    assert!(panes_width(h.state(), window) <= window * 0.6 + 0.5);
    let edge_now = h.get_by_label(EDGE_NAME).rect().center();
    drag(&mut h, edge_now, egui::pos2(1190.0, edge.y));
    assert!(panes_width(h.state(), window) >= 240.0);
    // Properties taller, by the bar between them; then Effects.
    let split = h.get_by_label(SPLIT_NAME).rect().center();
    drag(&mut h, split, egui::pos2(split.x, split.y + 60.0));
    let more = h.state().settings.properties_share.unwrap_or_default();
    assert!(more > 0.64, "{more}");
    let split_now = h.get_by_label(SPLIT_NAME).rect().center();
    assert!((split_now.y - (split.y + 60.0)).abs() < 2.0, "the bar follows: {split_now:?}");
    drag(&mut h, split_now, egui::pos2(split.x, 5.0));
    let top = h.get_by_label(SPLIT_NAME).rect().top();
    assert!(top > 150.0, "Properties keeps some room: {top}");
    shot(&mut h, "panes-resized");
    // Junk in the settings is no trouble: the usual sizes.
    h.state_mut().settings.side_width = Some(f32::NAN);
    h.state_mut().settings.properties_share = Some(f32::INFINITY);
    h.run_steps(3);
    assert_eq!(panes_width(h.state(), window), PANEL_WIDTH);
    h.state_mut().settings.side_width = Some(-5.0);
    h.state_mut().settings.properties_share = Some(-3.0);
    h.run_steps(3);
    assert!(panes_width(h.state(), window) >= 240.0);
    h.get_by_label("Thickness");
    h.get_by_label("Border");
}

#[test]
fn the_tool_properties_change_the_colours_and_the_callout_form() {
    let mut ed = editor();
    let id = callout(&mut ed);
    let mut h = ready_tall(ed, Tool::Select);
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
    let mut h = ready_tall(ed, Tool::Draw(Kind::Text));
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

#[test]
fn with_room_to_spare_each_tool_has_its_name_under_it_and_the_name_picks_it() {
    let mut h = sized(1600.0);
    let named_top = h.state().view.canvas.min.y;
    // Under the Blur button, its name: a click there picks the tool as the button does.
    let blur = h.get_by_label("Blur (B)").rect();
    crate::tests::click_at(&mut h, egui::pos2(blur.center().x, blur.max.y + 7.0));
    assert_eq!(h.state().tool, Tool::Draw(Kind::Blur));
    shot(&mut h, "tool-bar-named");
    // Turned off (View, Tool Names), the bar is as it was, and the picture has the room.
    h.state_mut().settings.hide_tool_names = true;
    h.run_steps(3);
    let plain_top = h.state().view.canvas.min.y;
    assert!((named_top - plain_top - 14.0).abs() < 0.5, "{named_top} and {plain_top}");
    // A window with no room for all of it in one row (the one the editor opens in) has the tools
    // on a row of their own under the sides, their names still under them.
    let mut usual = sized(1320.0);
    let (arrow, open) = (usual.get_by_label("Arrow (A)").rect(), usual.get_by_label("Open (Ctrl+O)").rect());
    assert!((arrow.width() - crate::editor::BAR_BUTTON).abs() < 0.5);
    assert!(arrow.min.y > open.max.y, "under the sides: {arrow:?} {open:?}");
    assert!(usual.state().view.canvas.min.y >= arrow.max.y + 13.5, "room for the names under them");
    let blur = usual.get_by_label("Blur (B)").rect();
    crate::tests::click_at(&mut usual, egui::pos2(blur.center().x, blur.max.y + 7.0));
    assert_eq!(usual.state().tool, Tool::Draw(Kind::Blur));
    shot(&mut usual, "tool-bar-named-two-rows");
    // Too narrow even for that, the icons alone, at full size.
    let narrow = sized(800.0);
    let arrow = narrow.get_by_label("Arrow (A)").rect();
    assert!((arrow.width() - crate::editor::BAR_BUTTON).abs() < 0.5);
}

#[test]
fn how_words_look_is_set_with_buttons_and_sizes_are_whole_numbers() {
    let ed = editor();
    let mut h = ready(ed, Tool::Draw(Kind::Text));
    // They're under the looks, further down the panel than a small window shows.
    h.get_by_label("Italic").scroll_to_me();
    h.run_steps(3);
    let before = h.state().tool_style(Kind::Text);
    click(&mut h, "Bold");
    click(&mut h, "Italic");
    click(&mut h, "Against the right");
    let s = h.state().tool_style(Kind::Text);
    assert_eq!((s.bold, s.italic), (!before.bold, !before.italic));
    assert_eq!(s.align, screengrabcraft_model::TextAlign::Right);
    // Pressed again, as it was.
    click(&mut h, "Bold");
    assert_eq!(h.state().tool_style(Kind::Text).bold, before.bold);
    // The buttons are in one row, each big enough to hit.
    let (b, right) = (h.get_by_label("Bold").rect(), h.get_by_label("Against the right").rect());
    assert!((b.center().y - right.center().y).abs() < 0.5 && b.height() >= 27.5 && right.width() >= 27.5, "{b:?} {right:?}");
    // A size is said without a decimal nobody set.
    assert!(h.query_by_label_contains(".0 px").is_none(), "no \"22.0 px\"");
    shot(&mut h, "text-style-buttons");
}

#[test]
fn nothing_selected_the_panel_says_how_things_are_moved() {
    let ed = editor();
    let mut h = ready(ed, Tool::Select);
    for said in ["Click", "Drag", "Double-click", "Arrows", "Right-click"] {
        assert!(h.query_by_label(said).is_some(), "{said}");
    }
    shot(&mut h, "move-tips");
}

/// The Shape tool's picker draws triangles and stars; the Shape button keeps drawing them; an
/// arrow is bent by the handle in its middle, and its picker makes the next one curved.
#[test]
fn shapes_draw_triangles_and_stars_and_arrows_bend() {
    let mut h = ready_tall(editor(), Tool::Draw(Kind::Rect));
    click(&mut h, "Shape options");
    click(&mut h, "Triangle");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(h.state().tool, Tool::Draw(Kind::Polygon));
    let (a, b) = (at(&h, 30.0, 30.0), at(&h, 130.0, 120.0));
    drag(&mut h, a, b);
    let last = |h: &Harness<'_, Editor>| h.state().session.doc.objects.last().map(|o| o.shape.clone());
    assert!(matches!(last(&h), Some(Shape::Polygon { sides: 3, star: false, .. })), "{:?}", last(&h));
    h.state_mut().session.select(Vec::new());
    h.run_steps(2);
    click(&mut h, "Shape options");
    click(&mut h, "Star");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let (a, b) = (at(&h, 150.0, 30.0), at(&h, 250.0, 120.0));
    drag(&mut h, a, b);
    assert!(matches!(last(&h), Some(Shape::Polygon { sides: 5, star: true, .. })), "{:?}", last(&h));
    // Six points for the next one, from the picker.
    h.state_mut().session.select(Vec::new());
    h.run_steps(2);
    click(&mut h, "Shape options");
    click(&mut h, "6");
    shot(&mut h, "shape-picker-polygons");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!((h.state().tool_options(Kind::Polygon).sides, h.state().tool_options(Kind::Polygon).star), (Some(6), Some(true)));
    // Another tool and back: the Shape button still draws stars.
    click(&mut h, "Arrow (A)");
    click(&mut h, "Shape (R, O)");
    assert_eq!(h.state().tool, Tool::Draw(Kind::Polygon));
    // An arrow, bent by its middle handle: one undo step.
    click(&mut h, "Arrow (A)");
    let (a, b) = (at(&h, 40.0, 220.0), at(&h, 240.0, 220.0));
    drag(&mut h, a, b);
    let (m, up) = (at(&h, 140.0, 220.0), at(&h, 140.0, 170.0));
    drag(&mut h, m, up);
    let Some(Shape::Arrow { bend, .. }) = last(&h) else { panic!("an arrow") };
    assert!((bend - 0.25).abs() < 0.03, "bowed up a quarter of its length: {bend}");
    assert_eq!(h.state().session.undo_label(), Some("Resize"));
    // An open head, from the picker: on the arrow, and on the next ones.
    click(&mut h, "Shape options");
    click(&mut h, "Open V");
    shot(&mut h, "arrow-shape-options");
    assert!(matches!(last(&h), Some(Shape::Arrow { head: screengrabcraft_model::HeadShape::Open, .. })), "{:?}", last(&h));
    assert_eq!(h.state().tool_options(Kind::Arrow).head, Some(screengrabcraft_model::HeadShape::Open));
    // The picker straightens it.
    click(&mut h, "Straight");
    let Some(Shape::Arrow { bend, .. }) = last(&h) else { panic!("an arrow") };
    assert_eq!(bend, 0.0);
    // Undone, it is bent again, for the picture of it.
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state_mut().session.undo());
    h.run_steps(2);
    shot(&mut h, "polygons-and-curved-arrow");
}

/// A host that keeps pictures of your own as stamps in memory, with a picture on its clipboard.
#[derive(Clone, Default)]
struct StampHost(std::rc::Rc<std::cell::RefCell<Vec<(String, screengrabcraft_model::Raster)>>>);

impl crate::services::Services for StampHost {
    fn has_paths(&self) -> bool {
        false
    }

    fn paste_image(&self) -> Option<screengrabcraft_model::Raster> {
        // A small picture of two colours, wider than tall.
        let mut r = screengrabcraft_model::Raster::filled(60, 30, Color::rgb(0, 120, 200)).ok()?;
        for px in r.pixels_mut().as_chunks_mut::<4>().0.iter_mut().take(60 * 15) {
            *px = [250, 200, 0, 255];
        }
        Some(r)
    }

    fn keeps_own_stamps(&self) -> bool {
        true
    }

    fn own_stamps(&self) -> Vec<String> {
        self.0.borrow().iter().map(|(n, _)| n.clone()).collect()
    }

    fn own_stamp(&self, name: &str) -> Option<screengrabcraft_model::Raster> {
        self.0.borrow().iter().find(|(n, _)| n == name).map(|(_, r)| r.clone())
    }

    fn add_own_stamp(&self, png: &[u8]) -> Result<String, String> {
        let pic = screengrabcraft_render::codec::decode(png).map_err(|e| e.to_string())?;
        let name = format!("mine-{}", self.0.borrow().len() + 1);
        self.0.borrow_mut().push((name.clone(), pic));
        Ok(name)
    }

    fn remove_own_stamp(&self, name: &str) -> Result<(), String> {
        self.0.borrow_mut().retain(|(n, _)| n != name);
        Ok(())
    }
}

/// The stamp library: a search finds a face, which is placed in its own colour; the copied
/// picture is kept as a stamp of your own and placed as a picture; then taken out again.
#[test]
fn the_stamp_library_finds_stamps_and_keeps_your_pictures() {
    use screengrabcraft_model::StampKind;
    let host = StampHost::default();
    let mut ed = Editor::new(Box::new(host.clone()), screengrabcraft_model::EditorSettings::default());
    ed.session.replace(screengrabcraft_model::Document::new(screengrabcraft_model::Raster::filled(400, 300, Color::WHITE).unwrap()));
    ed.view.fit_pending = true;
    let mut h = ready_tall(ed, Tool::Draw(Kind::Stamp));
    for name in ["Arrow up", "Sad face", "Pencil", crate::stamps::SEARCH] {
        h.get_by_label(name);
    }
    click(&mut h, crate::stamps::SEARCH);
    h.event(egui::Event::Text("smile".into()));
    h.run_steps(3);
    assert_eq!(h.state().stamp_query, "smile");
    assert!(h.query_by_label("Sad face").is_none() && h.query_by_label("Arrow up").is_none(), "the search leaves the rest out");
    shot(&mut h, "stamp-library-search");
    click(&mut h, "Happy face");
    assert_eq!(h.state().stamp(), StampKind::Happy);
    let p = at(&h, 100.0, 100.0);
    crate::tests::click_at(&mut h, p);
    let o = h.state().session.doc.objects.last().cloned().unwrap();
    assert!(matches!(o.shape, Shape::Stamp { stamp: StampKind::Happy, .. }), "{:?}", o.shape);
    assert_eq!(o.style.fill, StampKind::Happy.color(), "in its own colour");
    // The copied picture kept, chosen, and placed as a picture, in proportion.
    // (The search still narrows the stamps, so your pictures are in sight under them.)
    h.state_mut().session.select(Vec::new());
    h.run_steps(2);
    click(&mut h, "Add the Copied Picture");
    assert_eq!(crate::services::Services::own_stamps(&host), vec!["mine-1".to_string()]);
    assert_eq!(h.state().own_stamp.as_deref(), Some("mine-1"));
    h.get_by_label(&crate::stamps::own_name(0));
    let p = at(&h, 250.0, 150.0);
    crate::tests::click_at(&mut h, p);
    let o = h.state().session.doc.objects.last().cloned().unwrap();
    let Shape::Image { rect, image } = o.shape else { panic!("a picture: {:?}", o.shape) };
    assert!((rect.w / rect.h - 2.0).abs() < 0.05 && rect.contains(screengrabcraft_model::Pt::new(250.0, 150.0)), "{rect:?}");
    assert_eq!(h.state().session.doc.images.get(&image).map(|r| (r.width(), r.height())), Some((60, 30)));
    assert_eq!(h.state().session.undo_label(), Some("Add Stamp"));
    shot(&mut h, "stamp-library-own");
    // Let go of it, then the picture is taken out of the stamps.
    h.state_mut().session.select(Vec::new());
    h.run_steps(2);
    click(&mut h, &crate::stamps::own_name(0));
    click(&mut h, "Remove");
    assert!(crate::services::Services::own_stamps(&host).is_empty() && h.state().own_stamp.is_none());
    assert!(h.query_by_label(&crate::stamps::own_name(0)).is_none());
}

/// Where the host keeps no files (the browser), your pictures are greyed out with the reason.
#[test]
fn stamps_of_your_own_are_greyed_out_without_files() {
    let mut h = ready_tall(editor(), Tool::Draw(Kind::Stamp));
    h.get_by_label_contains("not in the browser");
    assert!(egui_kittest::kittest::NodeT::accesskit_node(&h.get_by_label("Add a Picture…")).is_disabled());
    shot(&mut h, "stamp-library-browser");
}

/// A computer with one font, "Test Mono" (the bundled typewriter's file under another name).
struct OneFont;

impl screengrabcraft_render::text::FontSource for OneFont {
    fn face(&self, family: &str, _bold: bool) -> Option<(Vec<u8>, u32)> {
        let mono = screengrabcraft_render::text::font_files().into_iter().find(|(n, _)| *n == "mono").map(|(_, b)| b.to_vec());
        if family == "Test Mono" { mono.map(|b| (b, 0)) } else { None }
    }

    fn families(&self) -> Vec<String> {
        vec!["Test Mono".to_string(), "Unused Serif".to_string()]
    }
}

/// The Text tool's font menu lists the computer's fonts with a search; text typed in one is
/// shown in it as it is typed, and keeps its name.
#[test]
fn text_takes_a_font_from_the_computer() {
    screengrabcraft_render::text::set_font_source(Box::new(OneFont));
    let mut h = ready_tall(editor(), Tool::Draw(Kind::Text));
    click(&mut h, crate::props::FONT_MENU);
    h.get_by_label(crate::props::BUNDLED_FONT);
    h.get_by_label("Unused Serif");
    click(&mut h, crate::props::FONT_SEARCH);
    h.event(egui::Event::Text("mono".into()));
    h.run_steps(3);
    assert!(h.query_by_label("Unused Serif").is_none(), "the search leaves it out");
    shot(&mut h, "font-menu");
    click(&mut h, "Test Mono");
    assert_eq!(h.state().tool_style(Kind::Text).font.as_str(), "Test Mono");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let p = at(&h, 60.0, 60.0);
    crate::tests::click_at(&mut h, p);
    h.event(egui::Event::Text("iiii".into()));
    h.run_steps(4);
    assert!(h.state().egui_fonts.contains("computer:Test Mono:bold"), "handed to egui to type in (text is bold at first)");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    let o = h.state().session.doc.objects.last().cloned().unwrap();
    assert_eq!((o.shape.text(), o.style.font.as_str()), (Some("iiii"), "Test Mono"));
    shot(&mut h, "font-text");
}

/// The effects added later are in the pane too: the colour changes keep to the area selected on
/// the picture, perspective tilts it, a watermark of words or of a chosen picture, the capture's
/// details.
#[test]
fn more_effects_in_the_pane_and_the_colours_keep_to_an_area() {
    use screengrabcraft_model::{Document, ImageFormat, Raster};
    let mut ed = editor();
    ed.session.replace(Document::new(Raster::filled(400, 300, Color::rgb(200, 40, 40)).unwrap()));
    ed.session.doc.meta.source = Some("A window".into());
    // Effects with most of the height, so an opened one is in sight.
    ed.settings.properties_share = Some(0.2);
    let mut h = crate::tests::harness_sized(ed, egui::vec2(1200.0, 1080.0));
    h.run_steps(4);
    for name in [
        "Brightness and Contrast",
        "Hue, Saturation and Gamma",
        crate::effects::COLOURS,
        "Sharpen",
        "Blur",
        "Border",
        "Drop Shadow",
        "Round Corners",
        "Torn Edge",
        "Wavy Edge",
        "Faded Edge",
        "Bevel",
        "Perspective",
        "Page Curl",
        "Reflection",
        "Watermark",
        "Capture Details",
    ] {
        h.get_by_label(name);
    }
    // An area selected: Sepia browns it alone.
    h.state_mut().area = Some(Rect::new(20.0, 20.0, 100.0, 80.0));
    click(&mut h, crate::effects::COLOURS);
    h.run_steps(20);
    shot(&mut h, "effects-pane-area");
    click(&mut h, "Sepia");
    let base = h.state().session.doc.base.clone();
    assert_eq!(h.state().session.undo_label(), Some("Sepia"));
    assert_ne!(base.pixel(50, 50), Some(Color::rgb(200, 40, 40)), "in the area");
    assert_eq!(base.pixel(300, 200), Some(Color::rgb(200, 40, 40)), "not outside it");
    click(&mut h, crate::effects::COLOURS);
    h.state_mut().area = None;
    // A watermark of words, at its settings as they start.
    click(&mut h, "Watermark");
    h.run_steps(20);
    shot(&mut h, "effects-pane-watermark");
    click(&mut h, "Apply Watermark");
    assert_eq!(h.state().session.undo_label(), Some("Watermark"));
    // Of a picture: chosen, it is named in the section and used.
    let logo = Raster::filled(20, 20, Color::rgb(0, 0, 255)).unwrap();
    let bytes = screengrabcraft_render::encode(&logo, ImageFormat::Png, 92).unwrap();
    let ctx = h.ctx.clone();
    h.state_mut().watermark_file(&ctx, &crate::services::OpenedFile { name: "logo.png".into(), path: None, bytes });
    h.run_steps(3);
    h.get_by_label("logo.png");
    let before = h.state().session.doc.base.pixel(200, 150).unwrap();
    click(&mut h, "Apply Watermark");
    assert!(h.state().session.lent.is_empty(), "lent only while it is applied");
    let mid = h.state().session.doc.base.pixel(200, 150).unwrap();
    assert!(mid.b > before.b + 30 && mid.r < before.r, "the blue picture is in the middle: {before:?} → {mid:?}");
    click(&mut h, "Watermark");
    // The capture's details, with what the picture is of.
    click(&mut h, "Capture Details");
    h.run_steps(20);
    shot(&mut h, "effects-pane-details");
    click(&mut h, "Apply Capture Details");
    assert_eq!(h.state().session.undo_label(), Some("Capture Details"));
    click(&mut h, "Capture Details");
    // Perspective changes the picture's size.
    click(&mut h, "Perspective");
    h.run_steps(20);
    click(&mut h, "Apply Perspective");
    assert!(h.state().session.doc.width() < 400);
    click(&mut h, "Perspective");
    h.run_steps(10);
    shot(&mut h, "effects-applied");
    // The Effects menu has them all.
    click(&mut h, "Effects");
    for name in ["Hue, Saturation and Gamma…", "Sepia", "Wavy Edge…", "Page Curl…", "Reflection…", "Watermark…", "Capture Details…"] {
        h.get_by_label(name);
    }
    shot(&mut h, "effects-menu");
}
