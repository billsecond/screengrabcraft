//! Grab Text (Image menu, the right-click menu, Ctrl+Shift+T): the text in the picture, or in the
//! area selected with the Selection tool, read on this computer and shown in a window to copy.
//!
//! The host lends the reader ([`crate::services::Services::text_reader`]); where there is none
//! (the browser, a system without one) the item is disabled and says why. It reads the picture
//! as it is shown, annotations included (so what is blurred stays unread). Reading takes a
//! moment, so it works on a thread of its own while a window says so.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::Ui;
use screengrabcraft_model::{Raster, Rect};

use crate::editor::Editor;
use crate::services::{ReadText, TextLine};

/// What the editor keeps for this: the reader at work, and the text read.
#[derive(Default)]
pub struct State {
    job: Option<Receiver<Result<Vec<TextLine>, String>>>,
    shown: Option<Shown>,
}

/// The text read, in the window (it can be put right before it's copied).
struct Shown {
    text: String,
    lines: usize,
    /// What was read: the selected area, or all of the picture.
    of_area: bool,
}

impl State {
    /// Whether a window of this has the editor's attention (its keys are left alone meanwhile).
    pub fn busy(&self) -> bool {
        self.job.is_some() || self.shown.is_some()
    }

    /// The text in the window, if it's open.
    pub fn text(&self) -> Option<&str> {
        self.shown.as_ref().map(|s| s.text.as_str())
    }
}

/// The lines read, one to a line.
pub fn joined(lines: &[TextLine]) -> String {
    let mut s = String::new();
    for l in lines {
        if !s.is_empty() {
            s.push('\n');
        }
        s.push_str(l.text.trim_end());
    }
    s
}

/// The Grab Text item (the Image menu and the right-click menu).
pub(crate) fn item(ed: &mut Editor, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let reader = ed.services.text_reader();
    let label = if ed.area.is_some() { "Grab Text from the Area" } else { "Grab Text" };
    let item = ui.add_enabled(reader.is_ok() && !ed.grab_text.busy(), egui::Button::new(label).shortcut_text("Ctrl+Shift+T"));
    let item = match &reader {
        Ok(_) => item.on_hover_text("Reads the text in the picture so you can copy it"),
        Err(why) => item.on_disabled_hover_text(why.as_str()),
    };
    if item.clicked() {
        start(ed, &ctx);
        ui.close();
    }
}

/// What Grab Text reads: the picture as shown (annotations drawn in), or the selected area of it.
fn picture(ed: &Editor) -> Option<(Raster, bool)> {
    let whole = screengrabcraft_render::render(&ed.session.doc)?;
    match ed.area.and_then(|a| a.intersect(ed.session.doc.bounds())) {
        Some(a) => {
            let (x, y) = (a.x.floor().max(0.0) as u32, a.y.floor().max(0.0) as u32);
            let (w, h) = (a.w.ceil().max(1.0) as u32, a.h.ceil().max(1.0) as u32);
            let w = w.min(whole.width().saturating_sub(x)).max(1);
            let h = h.min(whole.height().saturating_sub(y)).max(1);
            Some((whole.crop(x, y, w, h)?, true))
        }
        None => Some((whole, false)),
    }
}

/// Reads the text of the picture (or the area) on a thread of its own.
pub(crate) fn start(ed: &mut Editor, ctx: &egui::Context) {
    if ed.grab_text.busy() {
        return;
    }
    let reader: Arc<dyn ReadText> = match ed.services.text_reader() {
        Ok(r) => r,
        Err(why) => {
            ed.error(ctx, why);
            return;
        }
    };
    let Some((pic, of_area)) = picture(ed) else {
        ed.error(ctx, "Grab Text: the picture couldn't be drawn");
        return;
    };
    let (tx, rx) = channel();
    let wake = ctx.clone();
    let spawned = std::thread::Builder::new().name("grab-text".to_string()).spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reader.read(&pic)))
            .unwrap_or_else(|_| Err("the text reader failed unexpectedly".to_string()));
        let _ = tx.send(result);
        wake.request_repaint();
    });
    match spawned {
        Ok(_) => {
            ed.grab_text.job = Some(rx);
            ed.grab_text.shown = None;
            ed.grab_text_area = of_area;
        }
        Err(e) => ed.error(ctx, format!("Grab Text couldn't start: {e}")),
    }
}

/// Every frame: the reader's answer when it comes, and the window.
pub(crate) fn show(ed: &mut Editor, ctx: &egui::Context) {
    finish(ed, ctx);
    if ed.grab_text.job.is_some() {
        egui::Modal::new(egui::Id::new("grabbing-text")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.heading("Grab Text");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading the text…");
            });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    window(ed, ctx);
}

fn finish(ed: &mut Editor, ctx: &egui::Context) {
    let result = match ed.grab_text.job.as_ref().map(Receiver::try_recv) {
        None | Some(Err(TryRecvError::Empty)) => return,
        Some(Ok(r)) => r,
        Some(Err(TryRecvError::Disconnected)) => Err("the text reader stopped without an answer".to_string()),
    };
    ed.grab_text.job = None;
    match result {
        Ok(lines) if lines.iter().all(|l| l.text.trim().is_empty()) => {
            let what = if ed.grab_text_area { "the area" } else { "the picture" };
            ed.toast(ctx, format!("No text was found in {what}"));
        }
        Ok(lines) => ed.grab_text.shown = Some(Shown { text: joined(&lines), lines: lines.len(), of_area: ed.grab_text_area }),
        Err(e) => ed.error(ctx, format!("Grab Text: {e}")),
    }
}

fn window(ed: &mut Editor, ctx: &egui::Context) {
    let Some(mut shown) = ed.grab_text.shown.take() else { return };
    let mut open = true;
    let (mut copy, mut close) = (false, false);
    let size = ctx.content_rect().size();
    egui::Window::new("Grab Text")
        .collapsible(false)
        .resizable(true)
        .default_size([(size.x * 0.5).clamp(320.0, 640.0), (size.y * 0.5).clamp(200.0, 480.0)])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            let what = if shown.of_area { "the area" } else { "the picture" };
            let lines = if shown.lines == 1 { "1 line".to_string() } else { format!("{} lines", shown.lines) };
            ui.label(format!("{lines} read from {what}. Put anything right before you copy it."));
            ui.add_space(6.0);
            let room = (ui.available_height() - 48.0).max(80.0);
            egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut shown.text).desired_width(f32::INFINITY).desired_rows(6).id_salt("grabbed-text"));
            });
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() {
                    close = true;
                }
                if ui.add(egui::Button::new("Copy").min_size(egui::vec2(72.0, 0.0))).clicked() {
                    copy = true;
                }
            });
        });
    if copy {
        ctx.copy_text(shown.text.clone());
        ed.toast(ctx, "The text is on the clipboard");
        return;
    }
    if open && !close && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ed.grab_text.shown = Some(shown);
    }
}

/// Where a line read off an area is on the whole picture.
pub fn on_picture(line: &TextLine, area: Option<Rect>) -> Rect {
    match area {
        Some(a) => Rect::new(line.rect.x + a.x.floor(), line.rect.y + a.y.floor(), line.rect.w, line.rect.h),
        None => line.rect,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use screengrabcraft_model::{Color, Document, EditorSettings, Raster, Rect};

    use crate::editor::Editor;
    use crate::services::{NoServices, ReadText, Services, TextLine};

    /// Reads every picture as two lines, and says how large the picture was.
    struct Fake;

    impl ReadText for Fake {
        fn read(&self, picture: &Raster) -> Result<Vec<TextLine>, String> {
            Ok(vec![
                TextLine { text: "Order 4021".to_string(), rect: Rect::new(1.0, 2.0, 30.0, 8.0) },
                TextLine { text: format!("{}x{}", picture.width(), picture.height()), rect: Rect::new(1.0, 12.0, 30.0, 8.0) },
            ])
        }
    }

    struct Blank;

    impl ReadText for Blank {
        fn read(&self, _picture: &Raster) -> Result<Vec<TextLine>, String> {
            Ok(vec![TextLine { text: "  ".to_string(), rect: Rect::new(0.0, 0.0, 1.0, 1.0) }])
        }
    }

    struct With(Arc<dyn ReadText>);

    impl Services for With {
        fn text_reader(&self) -> Result<Arc<dyn ReadText>, String> {
            Ok(self.0.clone())
        }
    }

    fn harness(reader: Arc<dyn ReadText>) -> Harness<'static, Editor> {
        let mut ed = Editor::new(Box::new(With(reader)), EditorSettings::default());
        ed.session.replace(Document::new(Raster::filled(200, 100, Color::WHITE).unwrap()));
        Harness::builder().with_size(egui::vec2(1100.0, 760.0)).build_ui_state(
            |ui, ed: &mut Editor| {
                let ctx = ui.ctx().clone();
                ed.logic(&ctx);
                ed.ui(ui);
            },
            ed,
        )
    }

    fn until_read(h: &mut Harness<'_, Editor>) {
        for _ in 0..200 {
            h.step();
            if h.state().grab_text.job.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the text was never read");
    }

    #[test]
    fn grab_text_reads_the_picture_and_copies_it() {
        let mut h = harness(Arc::new(Fake));
        h.run_steps(3);
        let ctx = h.ctx.clone();
        super::start(h.state_mut(), &ctx);
        until_read(&mut h);
        h.run_steps(3);
        assert_eq!(h.state().grab_text.text(), Some("Order 4021\n200x100"));
        h.get_by_label("Copy").click();
        h.run_steps(3);
        assert!(h.state().grab_text.text().is_none(), "the window closes once copied");
        let copied = h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t.contains("Order 4021")));
        assert!(copied || h.state().toast_text().is_some_and(|t| t.contains("clipboard")));
    }

    #[test]
    fn grab_text_reads_only_the_selected_area() {
        let mut h = harness(Arc::new(Fake));
        h.run_steps(3);
        h.state_mut().area = Some(Rect::new(10.0, 10.0, 50.0, 40.0));
        let ctx = h.ctx.clone();
        super::start(h.state_mut(), &ctx);
        until_read(&mut h);
        h.run_steps(3);
        assert_eq!(h.state().grab_text.text(), Some("Order 4021\n50x40"));
        let line = TextLine { text: "x".to_string(), rect: Rect::new(1.0, 2.0, 3.0, 4.0) };
        assert_eq!(super::on_picture(&line, Some(Rect::new(10.0, 10.0, 50.0, 40.0))), Rect::new(11.0, 12.0, 3.0, 4.0));
    }

    #[test]
    fn a_picture_without_text_says_so() {
        let mut h = harness(Arc::new(Blank));
        h.run_steps(3);
        let ctx = h.ctx.clone();
        super::start(h.state_mut(), &ctx);
        until_read(&mut h);
        h.run_steps(3);
        assert!(h.state().grab_text.text().is_none());
        assert!(h.state().toast_text().is_some_and(|t| t.contains("No text")), "{:?}", h.state().toast_text());
    }

    #[test]
    fn without_a_reader_it_says_why() {
        let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
        ed.session.replace(Document::new(Raster::filled(20, 20, Color::WHITE).unwrap()));
        assert!(ed.services.text_reader().is_err());
        let ctx = egui::Context::default();
        super::start(&mut ed, &ctx);
        assert!(!ed.grab_text.busy());
    }
}
