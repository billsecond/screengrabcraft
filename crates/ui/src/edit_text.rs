//! Edit Text in the Picture (Image menu, the right-click menu): Grab Text's reader finds the
//! lines of text in the picture; each is outlined on it, and a click on one covers it with the
//! colour round it and puts a text annotation in its place, as large and in the colour it was,
//! ready to type into (`image.editText`). Esc, the right button or a click off the lines stops.
//!
//! Where the host has no reader (the browser, a system without one) the item is disabled and says
//! why. The font isn't recognised: the text is set in the computer's own interface font where it
//! has one (most screens are written in it), else in the built-in one.

use egui::{CursorIcon, Key, Modifiers, Stroke, StrokeKind, Ui};
use screengrabcraft_model::{ObjectId, TextLine};
use serde_json::{Value, json};

use crate::editor::{Editor, Tool};
use crate::grab_text::Reading;
use crate::theme::Tokens;

/// What the editor keeps for this: the reader at work, and the lines offered to pick from.
#[derive(Default)]
pub struct State {
    job: Option<Reading>,
    lines: Option<Vec<TextLine>>,
}

impl State {
    /// Whether the reader is at work.
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Whether the lines found are offered on the picture.
    pub fn picking(&self) -> bool {
        self.lines.is_some()
    }

    /// Stops offering the lines.
    pub fn stop(&mut self) {
        self.lines = None;
    }
}

/// The fonts screens are most often written in, the first the computer has being used.
const INTERFACE_FONTS: &[&str] =
    &["Segoe UI", "Segoe UI Variable Text", "SF Pro Text", "Helvetica Neue", "Cantarell", "Ubuntu", "Noto Sans", "DejaVu Sans"];

/// The Edit Text in the Picture item (the Image menu and the right-click menu).
pub(crate) fn item(ed: &mut Editor, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let reader = ed.services.text_reader();
    let item = ui.add_enabled(reader.is_ok() && !ed.edit_text.busy() && !ed.grab_text.busy(), egui::Button::new("Edit Text in the Picture…"));
    let item = match &reader {
        Ok(_) => item.on_hover_text("Finds the lines of text; click one to type over it in the same size and colour"),
        Err(why) => item.on_disabled_hover_text(why.replace("Grab Text", "Edit Text in the Picture")),
    };
    if item.clicked() {
        start(ed, &ctx);
        ui.close();
    }
}

/// Reads the text of the picture (or the area selected) on a thread of its own.
pub(crate) fn start(ed: &mut Editor, ctx: &egui::Context) {
    if ed.edit_text.busy() {
        return;
    }
    ed.stop_editing();
    match Reading::start(ed, ctx) {
        Ok(job) => {
            ed.edit_text.job = Some(job);
            ed.edit_text.lines = None;
        }
        Err(why) => ed.error(ctx, format!("Edit Text in the Picture: {why}")),
    }
}

/// Every frame: the reader's answer when it comes, and a window while it reads.
pub(crate) fn show(ed: &mut Editor, ctx: &egui::Context) {
    if let Some(job) = &ed.edit_text.job
        && let Some(result) = job.poll()
    {
        let of_area = job.area.is_some();
        ed.edit_text.job = None;
        match result {
            Ok(lines) => {
                let lines: Vec<TextLine> = lines.into_iter().filter(|l| !l.text.trim().is_empty()).collect();
                if lines.is_empty() {
                    ed.toast(ctx, format!("No text was found in {}", if of_area { "the area" } else { "the picture" }));
                } else {
                    ed.area = None;
                    ed.chosen = None;
                    ed.session.select(Vec::new());
                    ed.edit_text.lines = Some(lines);
                    ed.toast(ctx, "Click a line of text to change it (Esc to stop)");
                }
            }
            Err(e) => ed.error(ctx, format!("Edit Text in the Picture: {e}")),
        }
    }
    if ed.edit_text.job.is_some() {
        egui::Modal::new(egui::Id::new("finding-text")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.heading("Edit Text in the Picture");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Finding the text…");
            });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// The line under `p` (in the picture's pixels), if one is.
fn under(lines: &[TextLine], p: screengrabcraft_model::Pt) -> Option<usize> {
    lines.iter().position(|l| l.rect.inflate(3.0).contains(p))
}

/// The canvas's pointer while lines are offered: a click on one edits it; Esc, the right button
/// or a click off the lines stops. True when it took the pointer (nothing else happens then).
pub(crate) fn input(ed: &mut Editor, ctx: &egui::Context, resp: &egui::Response) -> bool {
    let Some(lines) = &ed.edit_text.lines else { return false };
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) || resp.secondary_clicked() {
        ed.edit_text.lines = None;
        return true;
    }
    let at = resp.hover_pos().map(|p| ed.view.to_image(p)).and_then(|p| under(lines, p));
    if at.is_some() {
        ctx.set_cursor_icon(CursorIcon::PointingHand);
    }
    if resp.clicked_by(egui::PointerButton::Primary) {
        match at.and_then(|i| lines.get(i)).cloned() {
            Some(line) => edit(ed, ctx, &line),
            None => ed.edit_text.lines = None,
        }
    }
    true
}

/// The lines offered, outlined on the picture; the one under the pointer filled.
pub(crate) fn paint(ed: &Editor, ui: &Ui, hover: Option<egui::Pos2>) {
    let Some(lines) = &ed.edit_text.lines else { return };
    let t = Tokens::get(ui.ctx());
    let at = hover.map(|p| ed.view.to_image(p)).and_then(|p| under(lines, p));
    let painter = ui.painter();
    for (i, l) in lines.iter().enumerate() {
        let r = ed.view.rect_to_screen(l.rect.inflate(2.0));
        if Some(i) == at {
            painter.rect_filled(r, 2.0, t.accent.gamma_multiply(0.25));
        }
        painter.rect_stroke(r, 2.0, Stroke::new(1.5, t.accent), StrokeKind::Outside);
    }
}

/// The line covered and a text annotation put in its place, to type into.
fn edit(ed: &mut Editor, ctx: &egui::Context, line: &TextLine) {
    ed.edit_text.lines = None;
    let mut p = json!({ "line": line });
    let fonts = ed.font_list.get_or_insert_with(|| screengrabcraft_render::text::font_families().unwrap_or_default());
    if let Some(font) = INTERFACE_FONTS.iter().find(|f| fonts.iter().any(|n| n.eq_ignore_ascii_case(f))) {
        p["font"] = json!(font);
    }
    let Some(out) = ed.run(ctx, "image.editText", p) else { return };
    if let Some(id) = out.get("text").and_then(Value::as_u64) {
        if !ed.tool.selects() {
            ed.set_tool(Tool::Select);
        }
        ed.start_editing(ObjectId(id));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use screengrabcraft_model::{Color, Document, EditorSettings, Raster, Rect, Shape, Style};

    use crate::editor::Editor;
    use crate::services::{NoServices, ReadText, Services, TextLine};

    /// A made-up price list: dark text on white, where the fake reader says it is.
    fn list() -> (Document, Vec<TextLine>) {
        let mut doc = Document::new(Raster::filled(360, 160, Color::WHITE).unwrap());
        let style = Style { font_size: 20.0, text_color: Color::rgb(40, 40, 40), stroke: Color::TRANSPARENT, ..Style::default() };
        let mut lines = Vec::new();
        for (y, t) in [(30.0, "Coffee 2.40"), (80.0, "Cake 3.10")] {
            doc.add(Shape::Text { rect: Rect::new(30.0, y, 300.0, 40.0), text: t.to_string() }, style).unwrap();
        }
        let shown = screengrabcraft_render::render(&doc).unwrap();
        // Each line's box: where its letters are.
        for (y0, y1, t) in [(30u32, 75u32, "Coffee 2.40"), (80, 125, "Cake 3.10")] {
            let (mut a, mut b, mut c, mut d) = (u32::MAX, u32::MAX, 0, 0);
            for y in y0..y1 {
                for x in 0..360 {
                    if shown.pixel(x, y).is_some_and(|p| p.r < 200) {
                        (a, b, c, d) = (a.min(x), b.min(y), c.max(x), d.max(y));
                    }
                }
            }
            lines.push(TextLine::new(t, Rect::new(a as f32, b as f32, (c - a + 1) as f32, (d - b + 1) as f32)));
        }
        (Document::new(shown), lines)
    }

    struct Fake(Vec<TextLine>);

    impl ReadText for Fake {
        fn read(&self, _picture: &Raster) -> Result<Vec<TextLine>, String> {
            Ok(self.0.clone())
        }
    }

    struct With(Arc<dyn ReadText>);

    impl Services for With {
        fn text_reader(&self) -> Result<Arc<dyn ReadText>, String> {
            Ok(self.0.clone())
        }
    }

    fn harness(services: Box<dyn Services>, doc: Document) -> Harness<'static, Editor> {
        let mut ed = Editor::new(services, EditorSettings::default());
        ed.session.replace(doc);
        Harness::builder().with_size(egui::vec2(1100.0, 1000.0)).build_ui_state(
            |ui, ed: &mut Editor| {
                let ctx = ui.ctx().clone();
                ed.logic(&ctx);
                ed.ui(ui);
            },
            ed,
        )
    }

    #[test]
    fn a_line_picked_is_covered_and_typed_over_in_its_size_and_colour() {
        let (doc, lines) = list();
        let second = lines[1].rect;
        let mut h = harness(Box::new(With(Arc::new(Fake(lines)))), doc);
        h.run_steps(4);
        h.get_by_label("Image").click();
        h.run_steps(2);
        h.get_by_label("Edit Text in the Picture…").click();
        h.run_steps(2);
        for _ in 0..200 {
            h.step();
            if !h.state().edit_text.busy() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        h.run_steps(2);
        assert!(h.state().edit_text.picking());
        let c = crate::tests::at(&h, second.center().x, second.center().y);
        h.hover_at(c);
        h.run_steps(2);
        crate::quick_tests::shot(&mut h, "edit-text-lines");
        crate::tests::click_at(&mut h, c);
        h.run_steps(3);
        assert!(!h.state().edit_text.picking());
        let ed = h.state();
        let id = ed.editing.expect("typing into the new text");
        let o = ed.session.doc.get(id).unwrap();
        assert_eq!(o.shape.text(), Some("Cake 3.10"));
        assert!((o.style.font_size - 20.0).abs() < 2.5, "{}", o.style.font_size);
        assert!(o.style.text_color.r < 90, "{:?}", o.style.text_color);
        assert_eq!(ed.session.doc.objects.len(), 2, "the cover and the text");
        crate::quick_tests::shot(&mut h, "edit-text-typing");
    }

    #[test]
    fn esc_stops_offering_the_lines() {
        let (doc, lines) = list();
        let mut h = harness(Box::new(With(Arc::new(Fake(lines)))), doc);
        h.run_steps(3);
        let ctx = h.ctx.clone();
        super::start(h.state_mut(), &ctx);
        for _ in 0..200 {
            h.step();
            if h.state().edit_text.picking() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(h.state().edit_text.picking());
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(!h.state().edit_text.picking());
        assert!(h.state().session.doc.objects.is_empty());
    }

    #[test]
    fn without_a_reader_the_item_is_disabled() {
        let (doc, _) = list();
        let mut h = harness(Box::new(NoServices), doc);
        h.run_steps(3);
        h.get_by_label("Image").click();
        h.run_steps(2);
        h.get_by_label("Edit Text in the Picture…").click();
        h.run_steps(2);
        assert!(!h.state().edit_text.busy() && !h.state().edit_text.picking());
    }
}
