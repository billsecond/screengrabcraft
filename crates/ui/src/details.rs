//! Hide Personal Details (Image menu, the right-click menu): email addresses, phone numbers,
//! card numbers and lines like a postal address found in the picture (or the area selected) with
//! Grab Text's reader, then each covered with a blur, a pixelate or a solid box of its own
//! (`image.hideDetails`), left selected to move or delete.
//!
//! Where the host has no reader (the browser, a system without one) the item is disabled and says
//! why. Faces aren't found: there is no model for them here.

use egui::Ui;
use screengrabcraft_engine::details::{Cover, Detail};
use serde_json::{Value, json};

use crate::editor::{Editor, Tool};
use crate::grab_text::Reading;

/// What the editor keeps for this: the window's choices, and the reader at work.
pub struct State {
    /// The window is open.
    open: bool,
    /// The kinds to find, as [`Detail::ALL`].
    find: [bool; 4],
    cover: Cover,
    job: Option<Reading>,
}

impl Default for State {
    fn default() -> State {
        State { open: false, find: [true; 4], cover: Cover::Blur, job: None }
    }
}

impl State {
    /// Whether a window of this has the editor's attention (its keys are left alone meanwhile).
    pub fn busy(&self) -> bool {
        self.open || self.job.is_some()
    }

    /// Whether its window is open (for tests).
    pub fn is_open(&self) -> bool {
        self.open
    }
}

/// How a kind is named in the window and in what it says after.
fn names(d: Detail) -> (&'static str, &'static str, &'static str) {
    match d {
        Detail::Email => ("Email addresses", "email address", "email addresses"),
        Detail::Phone => ("Phone numbers", "phone number", "phone numbers"),
        Detail::Card => ("Card numbers", "card number", "card numbers"),
        Detail::Address => ("Addresses", "address", "addresses"),
    }
}

/// The Hide Personal Details item (the Image menu and the right-click menu).
pub(crate) fn item(ed: &mut Editor, ui: &mut Ui) {
    let reader = ed.services.text_reader();
    let label = if ed.area.is_some() { "Hide Personal Details in the Area…" } else { "Hide Personal Details…" };
    let item = ui.add_enabled(reader.is_ok() && !ed.details.busy() && !ed.grab_text.busy(), egui::Button::new(label));
    let item = match &reader {
        Ok(_) => item.on_hover_text("Finds email addresses, phone numbers, card numbers and addresses, and covers each one"),
        Err(why) => item.on_disabled_hover_text(why.replace("Grab Text", "Hide Personal Details")),
    };
    if item.clicked() {
        ed.details.open = true;
        ui.close();
    }
}

/// Every frame: the window, the reader's answer when it comes, and a window while it reads.
pub(crate) fn show(ed: &mut Editor, ctx: &egui::Context) {
    finish(ed, ctx);
    if ed.details.job.is_some() {
        egui::Modal::new(egui::Id::new("finding-details")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.heading("Hide Personal Details");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Looking for personal details…");
            });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    window(ed, ctx);
}

fn window(ed: &mut Editor, ctx: &egui::Context) {
    if !ed.details.open {
        return;
    }
    let mut open = true;
    let (mut hide, mut close) = (false, false);
    let in_area = ed.area.is_some();
    egui::Window::new("Hide Personal Details")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_max_width(360.0);
            let what = if in_area { "the area selected" } else { "the picture" };
            ui.label(format!(
                "Reads the text in {what} on this computer, and covers what it finds. Each cover stays an annotation, to move or delete."
            ));
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Find").strong());
            for (on, d) in ed.details.find.iter_mut().zip(Detail::ALL) {
                ui.checkbox(on, names(d).0);
            }
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Cover with").strong());
            ui.horizontal(|ui| {
                ui.selectable_value(&mut ed.details.cover, Cover::Blur, "Blur");
                ui.selectable_value(&mut ed.details.cover, Cover::Pixelate, "Pixelate");
                ui.selectable_value(&mut ed.details.cover, Cover::Box, "Solid box");
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Faces aren't found. Look the picture over before you share it.").small().weak());
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let any = ed.details.find.iter().any(|on| *on);
                if ui.add_enabled(any, egui::Button::new("Hide")).clicked() || (any && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    hide = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if hide {
        ed.details.open = false;
        match Reading::start(ed, ctx) {
            Ok(job) => ed.details.job = Some(job),
            Err(why) => ed.error(ctx, format!("Hide Personal Details: {why}")),
        }
        return;
    }
    if !open || close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ed.details.open = false;
    }
}

/// The parameters of `image.hideDetails` for these lines and the window's choices.
fn params(state: &State, lines: &[screengrabcraft_model::TextLine], area: Option<screengrabcraft_model::Rect>) -> Value {
    let find: Vec<&str> = Detail::ALL.iter().zip(state.find).filter(|(_, on)| *on).map(|(d, _)| d.name()).collect();
    let mut p = json!({ "lines": lines, "find": find, "cover": state.cover });
    if let (Some(a), Value::Object(map)) = (area, &mut p) {
        map.insert("area".to_string(), json!(a));
    }
    p
}

/// What was hidden, in words: "2 email addresses and a phone number".
fn said(out: &Value) -> String {
    let parts: Vec<String> = Detail::ALL
        .iter()
        .filter_map(|d| {
            let n = out.get(d.name()).and_then(Value::as_u64).unwrap_or(0);
            let (_, one, many) = names(*d);
            match n {
                0 => None,
                1 => Some(format!("{} {one}", if one.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" })),
                n => Some(format!("{n} {many}")),
            }
        })
        .collect();
    match parts.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn finish(ed: &mut Editor, ctx: &egui::Context) {
    let Some(job) = &ed.details.job else { return };
    let Some(result) = job.poll() else { return };
    let area = job.area;
    ed.details.job = None;
    let lines = match result {
        Ok(lines) => lines,
        Err(e) => {
            ed.error(ctx, format!("Hide Personal Details: {e}"));
            return;
        }
    };
    let what = if area.is_some() { "the area" } else { "the picture" };
    let p = params(&ed.details, &lines, area);
    let Some(out) = ed.run(ctx, "image.hideDetails", p) else { return };
    if out.get("hidden").and_then(Value::as_u64).unwrap_or(0) == 0 {
        ed.toast(ctx, format!("No personal details were found in {what}"));
        return;
    }
    // The covers are selected: shown with their handles, to move, resize or delete.
    ed.area = None;
    ed.chosen = None;
    if !ed.tool.selects() {
        ed.set_tool(Tool::Select);
    }
    ed.toast(ctx, format!("Covered {}. Each cover can be moved or deleted.", said(&out)));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use screengrabcraft_model::{Color, Document, EditorSettings, Raster, Rect, Shape, Style};
    use serde_json::json;

    use crate::editor::Editor;
    use crate::services::{NoServices, ReadText, Services, TextLine};

    /// Reads made-up details off any picture, where they are drawn in the test's picture.
    struct Fake;

    impl ReadText for Fake {
        fn read(&self, _picture: &Raster) -> Result<Vec<TextLine>, String> {
            Ok(vec![
                TextLine::new("Jo Bloggs", Rect::new(30.0, 30.0, 140.0, 24.0)),
                TextLine::new("jo.bloggs@example.com", Rect::new(30.0, 70.0, 300.0, 24.0)),
                TextLine::new("Tel (555) 123-4567", Rect::new(30.0, 110.0, 250.0, 24.0)),
                TextLine::new("221B Baker Street", Rect::new(30.0, 150.0, 240.0, 24.0)),
            ])
        }
    }

    struct With(Arc<dyn ReadText>);

    impl Services for With {
        fn text_reader(&self) -> Result<Arc<dyn ReadText>, String> {
            Ok(self.0.clone())
        }
    }

    /// A made-up contact card: the words drawn where the fake reader says they are.
    fn card() -> Document {
        let mut doc = Document::new(Raster::filled(420, 200, Color::WHITE).unwrap());
        let style = Style { font_size: 18.0, text_color: Color::rgb(30, 30, 30), stroke: Color::TRANSPARENT, ..Style::default() };
        for (y, t) in [(30.0, "Jo Bloggs"), (70.0, "jo.bloggs@example.com"), (110.0, "Tel (555) 123-4567"), (150.0, "221B Baker Street")] {
            doc.add(Shape::Text { rect: Rect::new(26.0, y - 4.0, 380.0, 30.0), text: t.to_string() }, style).unwrap();
        }
        let base = screengrabcraft_render::render(&doc).unwrap();
        Document::new(base)
    }

    fn harness(services: Box<dyn Services>) -> Harness<'static, Editor> {
        let mut ed = Editor::new(services, EditorSettings::default());
        ed.session.replace(card());
        Harness::builder().with_size(egui::vec2(1100.0, 1000.0)).build_ui_state(
            |ui, ed: &mut Editor| {
                let ctx = ui.ctx().clone();
                ed.logic(&ctx);
                ed.ui(ui);
            },
            ed,
        )
    }

    fn until_done(h: &mut Harness<'_, Editor>) {
        for _ in 0..200 {
            h.step();
            if h.state().details.job.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the text was never read");
    }

    #[test]
    fn personal_details_are_found_and_covered_from_the_image_menu() {
        let mut h = harness(Box::new(With(Arc::new(Fake))));
        h.run_steps(4);
        h.get_by_label("Image").click();
        h.run_steps(2);
        h.get_by_label("Hide Personal Details…").click();
        h.run_steps(3);
        assert!(h.state().details.is_open());
        crate::quick_tests::shot(&mut h, "hide-details-window");
        h.get_by_label("Pixelate").click();
        h.run_steps(1);
        h.get_by_label("Hide").click();
        h.run_steps(1);
        until_done(&mut h);
        h.run_steps(4);
        let objects = &h.state().session.doc.objects;
        assert_eq!(objects.len(), 3, "{objects:?}");
        assert!(objects.iter().all(|o| matches!(o.shape, Shape::Pixelate { .. })));
        assert_eq!(h.state().session.selected().len(), 3);
        let toast = h.state().toast_text().unwrap_or_default().to_string();
        assert!(toast.contains("an email address, a phone number and an address"), "{toast}");
        crate::quick_tests::shot(&mut h, "hide-details-done");
        // One undo takes every cover away.
        let ctx = h.ctx.clone();
        h.state_mut().run(&ctx, "edit.undo", json!({}));
        assert!(h.state().session.doc.objects.is_empty());
    }

    #[test]
    fn only_the_kinds_ticked_are_hidden_as_solid_boxes() {
        let mut h = harness(Box::new(With(Arc::new(Fake))));
        h.run_steps(3);
        h.state_mut().details.find = [false, true, false, false];
        h.state_mut().details.cover = super::Cover::Box;
        let ctx = h.ctx.clone();
        let job = super::Reading::start(h.state(), &ctx).unwrap();
        h.state_mut().details.job = Some(job);
        until_done(&mut h);
        let objects = &h.state().session.doc.objects;
        assert!(matches!(objects.as_slice(), [o] if matches!(o.shape, Shape::Rect { .. }) && o.style.fill == Color::BLACK), "{objects:?}");
    }

    #[test]
    fn without_a_reader_the_item_says_why() {
        let mut h = harness(Box::new(NoServices));
        h.run_steps(3);
        h.get_by_label("Image").click();
        h.run_steps(2);
        h.get_by_label("Hide Personal Details…").click();
        h.run_steps(2);
        assert!(!h.state().details.is_open(), "the item is disabled");
    }

    #[test]
    fn what_was_hidden_is_said_in_words() {
        assert_eq!(super::said(&json!({"email": 2, "phone": 1})), "2 email addresses and a phone number");
        assert_eq!(super::said(&json!({"email": 1, "phone": 0, "card": 1, "address": 3})), "an email address, a card number and 3 addresses");
        assert_eq!(super::said(&json!({})), "");
    }
}
