//! Simplify (Image menu): the picture's parts drawn over it as plain shapes (`image.simplify`):
//! flat boxes in their colours, grey bars for its lines of text. Where the host has a text
//! reader (Grab Text's), the lines it reads are the bars; elsewhere they are found from the marks
//! in the picture. The shapes are one group under the other annotations: Ungroup to change one,
//! Delete (or Undo) to have the picture back.

use egui::Ui;
use serde_json::{Value, json};

use crate::editor::Editor;
use crate::grab_text::Reading;

/// The reader at work for Simplify, if it is.
#[derive(Default)]
pub struct State {
    job: Option<Reading>,
}

impl State {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
}

/// The Simplify item (the Image menu).
pub(crate) fn item(ed: &mut Editor, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let label = if ed.area.is_some() { "Simplify the Area" } else { "Simplify" };
    if ui
        .add_enabled(!ed.simplify.busy() && !ed.grab_text.busy(), egui::Button::new(label))
        .on_hover_text("Draws the picture's parts as plain shapes: flat boxes in their colours, grey bars for the text")
        .clicked()
    {
        start(ed, &ctx);
        ui.close();
    }
}

/// Reads the text first where there is a reader; else simplifies at once.
pub(crate) fn start(ed: &mut Editor, ctx: &egui::Context) {
    if ed.simplify.busy() {
        return;
    }
    if ed.services.text_reader().is_ok() {
        match Reading::start(ed, ctx) {
            Ok(job) => {
                ed.simplify.job = Some(job);
                return;
            }
            Err(e) => log::warn!("Simplify: the text couldn't be read ({e}); the marks are used"),
        }
    }
    let area = ed.area.and_then(|a| a.intersect(ed.session.doc.bounds()));
    run(ed, ctx, None, area);
}

fn run(ed: &mut Editor, ctx: &egui::Context, lines: Option<Vec<screengrabcraft_model::TextLine>>, area: Option<screengrabcraft_model::Rect>) {
    let mut p = json!({ "read": false });
    if let Some(lines) = lines {
        p["lines"] = json!(lines);
    }
    if let Some(a) = area {
        p["area"] = json!(a);
    }
    let Some(out) = ed.run(ctx, "image.simplify", p) else { return };
    let n = out.get("ids").and_then(Value::as_array).map_or(0, Vec::len);
    ed.area = None;
    ed.chosen = None;
    ed.toast(ctx, format!("Simplified into {n} shapes, one group under the rest: Ungroup to change one, Undo to have the picture back"));
}

/// Every frame: the reader's answer when it comes, and a window while it reads.
pub(crate) fn show(ed: &mut Editor, ctx: &egui::Context) {
    if let Some(job) = &ed.simplify.job
        && let Some(result) = job.poll()
    {
        let area = job.area;
        ed.simplify.job = None;
        // Without the text read, the marks in the picture stand for it.
        let lines = result.map_err(|e| log::warn!("Simplify: the text couldn't be read ({e}); the marks are used")).ok();
        run(ed, ctx, lines, area);
    }
    if ed.simplify.job.is_some() {
        egui::Modal::new(egui::Id::new("simplifying")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.heading("Simplify");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading the text…");
            });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use screengrabcraft_model::{Color, Document, EditorSettings, Raster, Rect, Shape};

    use crate::editor::{Editor, Tool};
    use crate::services::{NoServices, ReadText, Services, TextLine};

    /// A made-up window: a light page, a dark title bar, a blue button, lines of "text".
    fn window() -> Document {
        let mut r = Raster::filled(480, 300, Color::rgb(245, 246, 248)).unwrap();
        let mut fill = |x0: u32, y0: u32, w: u32, h: u32, c: Color| {
            let width = r.width() as usize;
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    let i = (y as usize * width + x as usize) * 4;
                    r.pixels_mut()[i..i + 4].copy_from_slice(&[c.r, c.g, c.b, 255]);
                }
            }
        };
        fill(0, 0, 480, 36, Color::rgb(36, 40, 48));
        fill(12, 10, 16, 16, Color::rgb(250, 190, 0));
        fill(40, 60, 140, 220, Color::rgb(225, 230, 238));
        fill(330, 240, 120, 36, Color::rgb(30, 120, 230));
        for (y, words) in [(70u32, 4u32), (100, 3), (130, 4)] {
            for k in 0..words * 5 {
                fill(52 + k * 4 + (k / 5) * 4, y, 3, 11, Color::rgb(30, 30, 30));
            }
        }
        for (y, words) in [(70u32, 8u32), (96, 7), (122, 8), (148, 5)] {
            for k in 0..words * 5 {
                fill(200 + k * 4 + (k / 5) * 4, y, 3, 11, Color::rgb(50, 50, 50));
            }
        }
        Document::new(r)
    }

    struct Fake;

    impl ReadText for Fake {
        fn read(&self, _picture: &Raster) -> Result<Vec<TextLine>, String> {
            Ok(vec![TextLine::new("Read line", Rect::new(200.0, 70.0, 180.0, 11.0))])
        }
    }

    struct With(Arc<dyn ReadText>);

    impl Services for With {
        fn text_reader(&self) -> Result<Arc<dyn ReadText>, String> {
            Ok(self.0.clone())
        }
    }

    fn harness(services: Box<dyn Services>) -> Harness<'static, Editor> {
        let mut ed = Editor::new(services, EditorSettings::default());
        ed.session.replace(window());
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
    fn the_picture_is_simplified_into_plain_shapes_from_the_image_menu() {
        let mut h = harness(Box::new(NoServices));
        h.run_steps(4);
        h.get_by_label("Image").click();
        h.run_steps(2);
        h.get_by_label("Simplify").click();
        h.run_steps(4);
        let doc = &h.state().session.doc;
        assert!(doc.objects.len() >= 10, "{}", doc.objects.len());
        assert!(doc.objects.iter().all(|o| matches!(o.shape, Shape::Rect { .. }) && o.group.is_some()));
        // Seven lines of text, seven grey bars.
        let bars = doc.objects.iter().filter(|o| matches!(o.shape, Shape::Rect { radius, .. } if radius > 2.0)).count();
        assert_eq!(bars, 7);
        assert!(h.state().toast_text().is_some_and(|t| t.contains("Simplified")));
        crate::quick_tests::shot(&mut h, "simplified");
    }

    #[test]
    fn the_lines_read_are_the_bars_where_there_is_a_reader() {
        let mut h = harness(Box::new(With(Arc::new(Fake))));
        h.run_steps(3);
        let ctx = h.ctx.clone();
        super::start(h.state_mut(), &ctx);
        for _ in 0..200 {
            h.step();
            if !h.state().simplify.busy() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let doc = &h.state().session.doc;
        assert!(doc.objects.iter().any(|o| o.shape.rect().is_some_and(|r| (r.x - 200.0).abs() < 0.01 && (r.w - 180.0).abs() < 0.01)));
    }

    #[test]
    fn a_part_dragged_away_leaves_its_place_filled_when_asked() {
        let mut h = harness(Box::new(NoServices));
        h.run_steps(3);
        h.state_mut().set_tool(Tool::Area);
        h.state_mut().settings.fill_lifted = true;
        h.state_mut().area = Some(Rect::new(320.0, 232.0, 140.0, 52.0));
        h.run_steps(2);
        crate::quick_tests::shot(&mut h, "fill-where-moved-panel");
        let (a, b) = (crate::tests::at(&h, 390.0, 258.0), crate::tests::at(&h, 390.0, 180.0));
        crate::tests::drag(&mut h, a, b);
        h.run_steps(3);
        let base = &h.state().session.doc.base;
        assert_eq!(base.pixel(390, 258), Some(Color::rgb(245, 246, 248)), "filled, not see-through");
        crate::quick_tests::shot(&mut h, "fill-where-moved");
    }
}
