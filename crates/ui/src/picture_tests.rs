//! Gradient fills and themes of your own, driven headless the way a person uses them.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use screengrabcraft_model::{Color, Document, GradientKind, Kind, Raster, Rect, Shape, Style};

use crate::editor::{Editor, Tool};
use crate::quick_tests::shot;
use crate::tests::{at, click_at, editor, ready_tall};

fn click(h: &mut Harness<'_, Editor>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

/// Clicks the first of the nodes called `label` (a button whose menu, open, has its name at the top).
fn click_first(h: &mut Harness<'_, Editor>, label: &str) {
    if let Some(n) = h.get_all_by_label(label).next() {
        n.click();
    }
    h.run_steps(3);
}

/// The editor on a white picture with a blue square in the middle.
fn square_picture() -> Editor {
    let mut r = Raster::filled(400, 300, Color::WHITE).unwrap();
    for y in 100..200 {
        for x in 150..250 {
            let i = (y * 400 + x) * 4;
            r.pixels_mut()[i..i + 4].copy_from_slice(&[30, 90, 200, 255]);
        }
    }
    let mut ed = editor();
    ed.session.replace(Document::new(r));
    ed
}

/// A selected box's Fill menu has the gradient: linear (with its angle) or radial, a second
/// colour beside the fill, and the two swapped.
#[test]
fn a_shape_takes_a_gradient_from_its_fill_menu() {
    let mut ed = editor();
    let style = Style { fill: Color::rgb(0xE5, 0x39, 0x35), ..Kind::Rect.default_style() };
    let id = ed.session.doc.add(Shape::Rect { rect: Rect::new(40.0, 40.0, 220.0, 140.0), radius: 0.0 }, style).unwrap();
    ed.session.select(vec![id]);
    let mut h = ready_tall(ed, Tool::Select);
    assert!(h.query_by_label(crate::props::GRADIENT_TO).is_none(), "one colour: no second one");
    click(&mut h, "Fill colour");
    click(&mut h, "Linear");
    let g = h.state().session.doc.get(id).map(|o| o.style.gradient).unwrap();
    assert_eq!((g.kind, g.to, g.angle), (GradientKind::Linear, Color::WHITE, 90.0), "down to white at first");
    h.get_by_label("Angle");
    click(&mut h, "Radial");
    assert_eq!(h.state().session.doc.get(id).map(|o| o.style.gradient.kind), Some(GradientKind::Radial));
    click(&mut h, "Swap the two colours");
    let s = h.state().session.doc.get(id).map(|o| o.style).unwrap();
    assert_eq!((s.fill, s.gradient.to), (Color::WHITE, Color::rgb(0xE5, 0x39, 0x35)));
    // The second colour has a button of its own by the fill, and the tool keeps the gradient.
    h.get_by_label(crate::props::GRADIENT_TO);
    assert_eq!(h.state().tool_style(Kind::Rect).gradient.kind, GradientKind::Radial);
    shot(&mut h, "gradient-menu");
    click_first(&mut h, "Fill colour");
    shot(&mut h, "gradient-shape");
    // One colour again takes it away.
    click_first(&mut h, "Fill colour");
    click(&mut h, "One colour");
    assert!(h.state().session.doc.get(id).is_some_and(|o| o.style.gradient.is_none()));
}

/// The Fill tool fills the area clicked with a gradient, from its colour to the second one, down
/// the area (not the whole picture).
#[test]
fn the_fill_tool_fills_with_a_gradient() {
    let mut h = ready_tall(square_picture(), Tool::Fill);
    click(&mut h, "Linear");
    assert_eq!(h.state().settings.fill.gradient.kind, GradientKind::Linear);
    h.get_by_label(crate::props::GRADIENT_TO);
    let p = at(&h, 200.0, 150.0);
    click_at(&mut h, p);
    let ed = h.state();
    let (top, bottom) = (ed.session.doc.base.pixel(200, 100).unwrap(), ed.session.doc.base.pixel(200, 199).unwrap());
    assert!(top.g < 80 && bottom.g > 220, "from red at the top of the square to white at its bottom: {top:?} {bottom:?}");
    assert_eq!(ed.session.doc.base.pixel(20, 20), Some(Color::WHITE), "the white round it isn't touched");
    shot(&mut h, "gradient-fill-tool");
}

/// A theme of your own: made from the colours in use, named, saved in the settings, listed after
/// the built-in themes and its quick styles drawn in its colours; deleted from the same menu.
#[test]
fn a_theme_of_your_own_is_made_from_the_current_colours() {
    let mut ed = editor();
    let mine = Color::rgb(0x12, 0x34, 0x99);
    ed.settings.styles.insert(Kind::Callout, Style { fill: mine, ..Kind::Callout.default_style() });
    let mut h = ready_tall(ed, Tool::Draw(Kind::Callout));
    click(&mut h, crate::quick::THEME_MENU);
    for name in ["Soft", "Vivid", "Mono"] {
        h.get_by_label(name);
    }
    click(&mut h, crate::quick::NEW_THEME);
    let draft = h.state().theme_draft.clone().unwrap();
    assert_eq!(draft.name, "My theme");
    assert_eq!(draft.colors.first(), Some(&mine), "the callout's fill first");
    shot(&mut h, "theme-new");
    click(&mut h, crate::quick::THEME_NAME);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("Team".into()));
    h.run_steps(2);
    click(&mut h, crate::quick::SAVE_THEME);
    let ed = h.state();
    assert!(ed.theme_draft.is_none());
    assert_eq!(ed.settings.own_themes.first().map(|t| t.name.as_str()), Some("Team"));
    assert_eq!(ed.settings.quick_theme, "Team", "put in use");
    // Its quick styles are named for its colours and drawn in them.
    click(&mut h, "Blue, tail down left");
    assert_eq!(h.state().tool_style(Kind::Callout).fill, mine);
    shot(&mut h, "theme-own");
    // Listed after the built-in ones; picking another and back again works as for them.
    click(&mut h, crate::quick::THEME_MENU);
    click(&mut h, "Vivid");
    assert_eq!(h.state().settings.quick_theme, "Vivid");
    click(&mut h, crate::quick::THEME_MENU);
    click(&mut h, "Team");
    click(&mut h, crate::quick::THEME_MENU);
    click(&mut h, "Delete Team");
    assert!(h.state().settings.own_themes.is_empty());
    assert_eq!(h.state().settings.quick_theme().name, "Bold");
    // A built-in name isn't taken: the form says why and stays.
    click(&mut h, crate::quick::THEME_MENU);
    click(&mut h, crate::quick::NEW_THEME);
    click(&mut h, crate::quick::THEME_NAME);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("Soft".into()));
    h.run_steps(2);
    click(&mut h, crate::quick::SAVE_THEME);
    assert!(h.state().theme_draft.as_ref().is_some_and(|d| d.problem.is_some()));
    assert!(h.state().settings.own_themes.is_empty());
    click(&mut h, "Cancel");
    assert!(h.state().theme_draft.is_none());
}

/// The editor on a plain picture, with the Effects pane given most of the height.
fn effects_editor() -> Harness<'static, Editor> {
    let mut ed = editor();
    ed.session.replace(Document::new(Raster::filled(400, 300, Color::rgb(200, 40, 40)).unwrap()));
    ed.settings.properties_share = Some(0.2);
    let mut h = crate::tests::harness_sized(ed, egui::vec2(1200.0, 1080.0));
    h.run_steps(4);
    h
}

/// The ready-made presets are at the top of the Effects pane, each applied with one click as one
/// undo step; an effect's settings are kept as a preset of your own, applied and deleted.
#[test]
fn effect_presets_apply_with_one_click_and_are_kept_by_name() {
    let mut h = effects_editor();
    click(&mut h, crate::effects::PRESETS);
    h.run_steps(10);
    shot(&mut h, "effect-presets");
    click(&mut h, "Photo Frame");
    let ed = h.state();
    assert!(ed.session.doc.width() > 400 + 28 && ed.session.doc.height() > 300 + 28, "a border, then a shadow");
    let mid = ed.session.doc.width() / 2;
    assert!((0..60).any(|y| ed.session.doc.base.pixel(mid, y) == Some(Color::WHITE)), "the white frame");
    assert_eq!(ed.session.undo_label(), Some("Effect Preset"));
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.doc.width(), 400, "one step");
    // A border of your own, kept and applied.
    click(&mut h, "Border");
    h.run_steps(10);
    click_first(&mut h, crate::effects::KEEP_PRESET);
    click(&mut h, crate::effects::PRESET_NAME);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("Thick red".into()));
    h.run_steps(2);
    click(&mut h, "Keep");
    let kept = h.state().settings.effect_presets.first().cloned().unwrap();
    assert_eq!((kept.name.as_str(), kept.steps.first().map(|s| s.effect.as_str())), ("Thick red", Some("effect.border")));
    shot(&mut h, "effect-preset-kept");
    click(&mut h, "Thick red");
    assert!(h.state().session.doc.width() > 400);
    assert_eq!(h.state().session.undo_label(), Some("Effect Preset"));
    // A ready-made name isn't taken.
    click_first(&mut h, crate::effects::KEEP_PRESET);
    click(&mut h, crate::effects::PRESET_NAME);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("Old Photo".into()));
    h.run_steps(2);
    click(&mut h, "Keep");
    assert_eq!(h.state().settings.effect_presets.len(), 1);
    click(&mut h, "Cancel");
    // A right-click deletes one of your own.
    h.get_by_label("Thick red").click_secondary();
    h.run_steps(3);
    click(&mut h, "Delete Thick red");
    assert!(h.state().settings.effect_presets.is_empty());
}

/// Image › Rotate by an Angle… turns the picture by any angle, the corners see-through, the
/// annotations with it.
#[test]
fn the_picture_rotates_by_any_angle() {
    let mut ed = square_picture();
    ed.session.doc.add(Shape::Rect { rect: Rect::new(160.0, 110.0, 80.0, 80.0), radius: 0.0 }, Kind::Rect.default_style()).unwrap();
    let mut h = ready_tall(ed, Tool::Select);
    click(&mut h, "Image");
    click(&mut h, "Rotate by an Angle…");
    h.get_by_label("Angle");
    shot(&mut h, "rotate-any");
    click(&mut h, "Apply");
    let ed = h.state();
    assert!(ed.session.doc.width() > 400 && ed.session.doc.height() > 300, "grown to hold it");
    assert_eq!(ed.session.doc.base.pixel(0, 0).map(|c| c.a), Some(0), "see-through corners");
    assert!(ed.session.doc.objects.first().is_some_and(|o| (o.rotation - 15.0).abs() < 1e-3), "the box turned with it");
    assert!(ed.dialog.is_none());
    h.run_steps(4);
    shot(&mut h, "rotated");
}

/// View › Rulers and Guides: a guide dragged out of each ruler; a box moved near one snaps to it,
/// not with snapping off; a guide dragged back onto its ruler goes.
#[test]
fn guides_come_out_of_the_rulers_and_moved_boxes_snap() {
    use crate::guides::{Guide, LEFT_RULER, TOP_RULER};
    let mut ed = editor();
    ed.session.replace(Document::new(Raster::filled(400, 300, Color::WHITE).unwrap()));
    let filled = Style { fill: Color::rgb(30, 90, 200), ..Kind::Rect.default_style() };
    let id = ed.session.doc.add(Shape::Rect { rect: Rect::new(100.0, 100.0, 50.0, 50.0), radius: 0.0 }, filled).unwrap();
    ed.settings.show_rulers = true;
    let mut h = ready_tall(ed, Tool::Select);
    let top = h.get_by_label(TOP_RULER).rect().center();
    let to = at(&h, 250.0, 200.0);
    crate::tests::drag(&mut h, egui::pos2(to.x, top.y), to);
    let left = h.get_by_label(LEFT_RULER).rect().center();
    let to = at(&h, 300.0, 120.0);
    crate::tests::drag(&mut h, egui::pos2(left.x, to.y), to);
    let guides = h.state().guides.clone();
    assert_eq!(guides.len(), 2, "{guides:?}");
    assert!(guides.first().is_some_and(|g| g.across && (g.at - 200.0).abs() <= 1.0), "{guides:?}");
    assert!(guides.get(1).is_some_and(|g| !g.across && (g.at - 300.0).abs() <= 1.0), "{guides:?}");
    // The box's bottom (its outline's outer edge) dragged to just above the guide across: it
    // snaps onto it.
    let line = h.state().guides.first().map(|g| g.at).unwrap();
    let (a, b) = (at(&h, 125.0, 125.0), at(&h, 126.0, line - 29.0));
    crate::tests::drag(&mut h, a, b);
    let seen = h.state().session.doc.bounds_of(&[id]).unwrap();
    assert!((seen.bottom() - line).abs() < 0.01, "its bottom on the guide: {seen:?}");
    let r = h.state().session.doc.get(id).and_then(|o| o.shape.rect()).unwrap();
    assert!((r.x - 101.0).abs() < 0.6, "across, nothing near: {r:?}");
    shot(&mut h, "guides");
    // Snapping off: it goes where it is put.
    h.state_mut().settings.no_snap = true;
    let (a, b) = (at(&h, 120.0, 170.0), at(&h, 120.0, 168.0));
    crate::tests::drag(&mut h, a, b);
    let moved = h.state().session.doc.bounds_of(&[id]).unwrap();
    assert!((moved.bottom() - (line - 2.0)).abs() < 0.6, "{moved:?}");
    // A guide dragged back onto its ruler goes.
    let g = h.get_by_label_contains("Guide down at").rect().center();
    crate::tests::drag(&mut h, g, egui::pos2(left.x, g.y));
    assert_eq!(h.state().guides, vec![Guide { across: true, at: line }]);
    // Guides at the middle, from the View menu; Clear Guides takes them all away.
    click(&mut h, "View");
    click(&mut h, "Guides at the Middle");
    assert_eq!(h.state().guides.len(), 3);
    click(&mut h, "View");
    click(&mut h, "Clear Guides");
    assert!(h.state().guides.is_empty());
}

/// The Measure tool (I): a drag measures, the panel reads it out, and it is kept on the picture
/// as a grouped dimension line and label when asked, or every time.
#[test]
fn the_measure_tool_measures_a_distance() {
    let mut h = ready_tall(editor(), Tool::Select);
    h.key_press(egui::Key::I);
    h.run_steps(2);
    assert_eq!(h.state().tool, Tool::Measure);
    let (a, b) = (at(&h, 40.0, 60.0), at(&h, 160.0, 60.0));
    crate::tests::drag(&mut h, a, b);
    let (from, to) = h.state().measured.unwrap();
    assert!((from.distance(to) - 120.0).abs() < 1.0, "{from:?} {to:?}");
    assert!(h.state().session.doc.objects.is_empty(), "not kept unless asked");
    h.get_by_label("Length");
    shot(&mut h, "measure");
    click(&mut h, crate::props::KEEP_THIS);
    let doc = &h.state().session.doc;
    assert_eq!(doc.objects.len(), 2);
    assert!(doc.objects.iter().all(|o| o.group.is_some()));
    assert!(doc.objects.iter().any(|o| o.shape.text().is_some_and(|t| t.ends_with(" px"))));
    // Kept every time.
    click(&mut h, crate::props::KEEP_EACH);
    assert!(h.state().settings.measure.keep);
    let (a, b) = (at(&h, 40.0, 120.0), at(&h, 40.0, 220.0));
    crate::tests::drag(&mut h, a, b);
    assert_eq!(h.state().session.doc.objects.len(), 4);
    shot(&mut h, "measure-kept");
}

/// View › Customize Tool Bar: a tool left off and another moved up are kept in the settings and
/// shown so; the one left off is still picked by its key; Usual Tools puts the bar back.
#[test]
fn the_tool_bar_is_customized() {
    let mut h = crate::tests::harness_sized(editor(), egui::vec2(1700.0, 1000.0));
    h.run_steps(4);
    click(&mut h, "View");
    click(&mut h, "Customize Tool Bar…");
    h.run_steps(6);
    h.get_by_label(crate::toolbar::TITLE);
    click(&mut h, "Magnify");
    click(&mut h, "Move Hand up");
    shot(&mut h, "customize-tool-bar");
    click(&mut h, "Save");
    let kept = h.state().settings.tool_bar.clone();
    assert_eq!(kept.first().map(String::as_str), Some("hand"), "{kept:?}");
    assert!(!kept.iter().any(|t| t == "magnify"));
    assert!(h.state().customizing_bar.is_none());
    h.run_steps(2);
    assert!(h.query_by_label("Magnify (M)").is_none(), "left off the bar");
    let (hand, mv) = (h.get_by_label("Hand (D)").rect(), h.get_by_label("Move (V)").rect());
    assert!(hand.max.x <= mv.min.x, "Hand before Move");
    shot(&mut h, "tool-bar-customized");
    h.key_press(egui::Key::M);
    h.run_steps(2);
    assert_eq!(h.state().tool, Tool::Draw(Kind::Magnify), "still picked by its key");
    click(&mut h, "View");
    click(&mut h, "Customize Tool Bar…");
    click(&mut h, "Usual Tools");
    click(&mut h, "Save");
    assert!(h.state().settings.tool_bar.is_empty());
    h.get_by_label("Magnify (M)");
}

/// Presses and lets go of the right button at `p`.
fn right_click(h: &mut Harness<'_, Editor>, p: egui::Pos2) {
    h.hover_at(p);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(2);
}

/// The unknown words underlined while a text is typed, by where their line is.
fn underlined(h: &Harness<'_, Editor>) -> Vec<(String, egui::Rect)> {
    h.state().spell.shown.iter().map(|(m, r)| (m.word.clone(), *r)).collect()
}

/// While a text is typed, the words the word list doesn't know are underlined; a right click on
/// one offers corrections, which take its place with the typing going on, and Add to Dictionary,
/// kept in the settings; Check Spelling turns it all off. The whole typing is one undo step.
#[test]
fn misspelt_words_are_underlined_and_put_right_from_the_right_click_menu() {
    use crate::spell::{ADD_WORD, CHECK_SPELLING};
    assert!(crate::spell::ready_now(), "the word list reads");
    let mut ed = editor();
    let id = ed.session.doc.add(Shape::Text { rect: Rect::new(40.0, 40.0, 320.0, 60.0), text: String::new() }, Kind::Text.default_style()).unwrap();
    let mut h = crate::tests::harness(ed);
    h.run_steps(4);
    h.state_mut().start_editing(id);
    h.run_steps(2);
    h.event(egui::Event::Text("Teh cat sat on Zorblax".to_string()));
    h.run_steps(4);
    let words: Vec<String> = underlined(&h).into_iter().map(|(w, _)| w).collect();
    assert_eq!(words, ["Teh", "Zorblax"]);
    shot(&mut h, "spelling-underlined");
    // A right click on "Teh": its corrections, Add to Dictionary, Check Spelling.
    let teh = underlined(&h).first().map(|(_, r)| r.center()).unwrap();
    right_click(&mut h, teh);
    let menu = h.state().spell.menu.clone().expect("the spelling menu is open");
    let offered = menu.word.map(|(_, s)| s).unwrap_or_default();
    assert!(offered.iter().any(|s| s == "The"), "{offered:?}");
    h.get_by_label(ADD_WORD);
    h.get_by_label(CHECK_SPELLING);
    shot(&mut h, "spelling-menu");
    click(&mut h, "The");
    h.run_steps(3);
    let ed = h.state();
    assert!(ed.spell.menu.is_none(), "closed once one is taken");
    assert_eq!(ed.editing, Some(id), "the typing goes on");
    assert_eq!(ed.session.doc.get(id).and_then(|o| o.shape.text()), Some("The cat sat on Zorblax"));
    assert_eq!(underlined(&h).into_iter().map(|(w, _)| w).collect::<Vec<_>>(), ["Zorblax"]);
    // Typing goes on where the correction ended.
    h.event(egui::Event::Text("!".to_string()));
    h.run_steps(3);
    assert_eq!(h.state().edit_buffer, "The! cat sat on Zorblax");
    // Zorblax added to the dictionary: not underlined again, and kept.
    let zorblax = underlined(&h).first().map(|(_, r)| r.center()).unwrap();
    right_click(&mut h, zorblax);
    click(&mut h, ADD_WORD);
    h.run_steps(3);
    assert_eq!(h.state().settings.own_words, ["Zorblax"]);
    assert!(underlined(&h).is_empty());
    assert_eq!(h.state().editing, Some(id));
    // A right click elsewhere in the text has Check Spelling only; Esc closes the menu, not the box.
    // (The right click put the cursor in the word clicked: to the end first.)
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::End);
    h.run_steps(2);
    h.event(egui::Event::Text(" Wrnog".to_string()));
    h.run_steps(3);
    assert_eq!(underlined(&h).into_iter().map(|(w, _)| w).collect::<Vec<_>>(), ["Wrnog"], "{}", h.state().edit_buffer);
    // Where "Teh" was: "The" now, a word it knows.
    let start = teh;
    right_click(&mut h, start);
    assert!(h.state().spell.menu.as_ref().is_some_and(|m| m.word.is_none()), "{:?}", h.state().spell.menu);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().spell.menu.is_none());
    assert_eq!(h.state().editing, Some(id), "Esc closed the menu only");
    // Check Spelling, off from the menu: nothing is underlined, and the setting is kept.
    right_click(&mut h, start);
    click(&mut h, CHECK_SPELLING);
    h.run_steps(3);
    assert!(h.state().settings.no_spell_check);
    assert!(underlined(&h).is_empty());
    // Done typing: everything typed is one undo step.
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    let ed = h.state();
    assert!(ed.editing.is_none());
    assert_eq!(ed.session.doc.get(id).and_then(|o| o.shape.text()), Some("The! cat sat on Zorblax Wrnog"));
    assert_eq!(ed.session.undo_label(), Some("Type Text"));
    // And on again from the Edit menu.
    click(&mut h, "Edit");
    click(&mut h, CHECK_SPELLING);
    assert!(!h.state().settings.no_spell_check);
}
