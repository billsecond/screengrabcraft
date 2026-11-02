//! Making the background transparent, from the Image menu:
//!
//! - **Remove Background**: a model tells the subject from the background. It takes seconds, so
//!   it works on a thread of its own while a window shows how far it has got and offers to stop;
//!   its matte is then applied by `image.removeBackground`, one undo step. The host lends the
//!   model ([`crate::services::Services::background_model`]); where there is none (the browser)
//!   the item is disabled and says why.
//! - **Remove Solid Background**: the plain colour round the picture's edges, at once.
//! - **Make a Colour Transparent…**: a colour chosen, with a tolerance, everywhere or only where it
//!   reaches the edges.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::{Color32, Ui};
use screengrabcraft_model::{Color, Raster};
use screengrabcraft_render::matte::{self, Matte, MatteError, Matting};
use serde_json::json;

use crate::editor::Editor;

/// What the editor keeps for this: the model at work, and the colour window.
#[derive(Default)]
pub struct State {
    job: Option<Job>,
    colour: Option<ColourWindow>,
}

impl State {
    /// Whether a window of this has the editor's attention (its keys are left alone meanwhile).
    pub fn busy(&self) -> bool {
        self.job.is_some() || self.colour.is_some()
    }

    /// Whether a model is at work.
    pub fn working(&self) -> bool {
        self.job.is_some()
    }
}

/// A model at work on a picture.
struct Job {
    /// The picture it works on; its answer is used only if the picture is still this one.
    picture: Raster,
    model: String,
    /// How far it has got (0 to 1, as `f32` bits).
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
    answer: Receiver<Result<Matte, MatteError>>,
}

impl Drop for Job {
    fn drop(&mut self) {
        // A window closed or a job forgotten stops the model too.
        self.cancel.store(true, Ordering::Relaxed);
    }
}

struct ColourWindow {
    colour: Color32,
    tolerance: u8,
    edges_only: bool,
}

impl ColourWindow {
    /// Starting from the background's colour if it has a plain one, else the top left pixel's.
    fn new(picture: &Raster) -> ColourWindow {
        let c = matte::edge_colour(picture).filter(|c| c.a > 0).or_else(|| picture.pixel(0, 0)).unwrap_or(Color::WHITE);
        ColourWindow { colour: Color32::from_rgb(c.r, c.g, c.b), tolerance: 24, edges_only: false }
    }
}

/// The Image menu's items for the background.
pub(crate) fn menu(ed: &mut Editor, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let model = ed.services.background_model();
    let working = ed.background.working();
    let item = ui.add_enabled(model.is_ok() && !working, egui::Button::new("Remove Background"));
    let item = match &model {
        Ok(_) => item.on_hover_text("Tells the subject from the background and makes the background transparent (a few seconds)"),
        Err(why) => item.on_disabled_hover_text(why.as_str()),
    };
    if item.clicked() {
        if let Ok(m) = model {
            start(ed, &ctx, m);
        }
        ui.close();
    }
    if ui.button("Remove Solid Background").on_hover_text("Makes the plain colour round the picture's edges transparent, up to the subject").clicked()
    {
        ed.run(&ctx, "image.removeSolidBackground", json!({}));
        ui.close();
    }
    if ui.button("Make a Colour Transparent…").clicked() {
        ed.background.colour = Some(ColourWindow::new(&ed.session.doc.base));
        ui.close();
    }
}

/// Sets `model` to work on the picture as it is now, on a thread of its own.
pub(crate) fn start(ed: &mut Editor, ctx: &egui::Context, model: Arc<dyn Matting>) {
    if ed.background.working() {
        return;
    }
    let picture = ed.session.doc.base.clone();
    let name = model.name().to_string();
    let progress = Arc::new(AtomicU32::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, answer) = channel();
    let (seen, stop, wake, pic) = (progress.clone(), cancel.clone(), ctx.clone(), picture.clone());
    let spawned = std::thread::Builder::new().name("remove-background".to_string()).spawn(move || {
        let mut shown = u32::MAX;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            model.matte(&pic, &mut |f| {
                // The window is drawn again each whole per cent, not for every step of the model.
                let f = if f.is_finite() { f.clamp(0.0, 1.0) } else { 0.0 };
                let pct = (f * 100.0) as u32;
                if pct != shown {
                    shown = pct;
                    seen.store(f.to_bits(), Ordering::Relaxed);
                    wake.request_repaint();
                }
                !stop.load(Ordering::Relaxed)
            })
        }))
        .unwrap_or_else(|_| Err(MatteError::Model("the model failed unexpectedly".to_string())));
        let _ = tx.send(result);
        wake.request_repaint();
    });
    match spawned {
        Ok(_) => ed.background.job = Some(Job { picture, model: name, progress, cancel, answer }),
        Err(e) => ed.error(ctx, format!("Remove Background couldn't start: {e}")),
    }
}

/// Every frame: the model's answer when it comes, the window showing how far it has got, and the
/// colour window.
pub(crate) fn show(ed: &mut Editor, ctx: &egui::Context) {
    finish(ed, ctx);
    working_window(ed, ctx);
    colour_window(ed, ctx);
}

fn finish(ed: &mut Editor, ctx: &egui::Context) {
    let result = match ed.background.job.as_ref().map(|j| j.answer.try_recv()) {
        None | Some(Err(TryRecvError::Empty)) => return,
        Some(Ok(r)) => r,
        Some(Err(TryRecvError::Disconnected)) => Err(MatteError::Model("the model stopped without an answer".to_string())),
    };
    let Some(job) = ed.background.job.take() else { return };
    match result {
        Ok(found) if job.picture.same_pixels(&ed.session.doc.base) => {
            ed.session.background.prepare(job.picture.clone(), &job.model, found);
            if ed.run(ctx, "image.removeBackground", json!({ "model": job.model })).is_some() {
                ed.toast(ctx, "The background is transparent now (Undo puts it back)");
            }
        }
        Ok(_) => ed.error(ctx, "The picture changed while its background was being worked out, so it was left as it is"),
        Err(MatteError::Cancelled) => ed.toast(ctx, "Stopped: the picture is as it was"),
        Err(e) => ed.error(ctx, format!("Remove Background: {e}")),
    }
}

fn working_window(ed: &mut Editor, ctx: &egui::Context) {
    let Some(job) = ed.background.job.as_ref() else { return };
    let done = f32::from_bits(job.progress.load(Ordering::Relaxed));
    let done = if done.is_finite() { done.clamp(0.0, 1.0) } else { 0.0 };
    let stopping = job.cancel.load(Ordering::Relaxed);
    let mut stop = false;
    let modal = egui::Modal::new(egui::Id::new("removing-background")).show(ctx, |ui| {
        ui.set_width(360.0);
        ui.heading("Removing the background");
        ui.add_space(6.0);
        ui.label("Telling the subject from the background…");
        ui.label(egui::RichText::new("The first time takes longer, while the model is read.").small());
        ui.add_space(8.0);
        ui.add(egui::ProgressBar::new(done).show_percentage().animate(!stopping));
        ui.add_space(10.0);
        let label = if stopping { "Stopping…" } else { "Stop" };
        if ui.add_enabled(!stopping, egui::Button::new(label)).clicked() {
            stop = true;
        }
    });
    if stop || modal.should_close() {
        job.cancel.store(true, Ordering::Relaxed);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

fn colour_window(ed: &mut Editor, ctx: &egui::Context) {
    let Some(mut w) = ed.background.colour.take() else { return };
    let mut open = true;
    let (mut apply, mut close) = (false, false);
    egui::Window::new("Make a Colour Transparent")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            egui::Grid::new("colour-transparent").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Colour");
                egui::color_picker::color_edit_button_srgba(ui, &mut w.colour, egui::color_picker::Alpha::Opaque);
                ui.end_row();
                ui.label("Tolerance");
                ui.add(egui::Slider::new(&mut w.tolerance, 0..=128));
                ui.end_row();
            });
            ui.checkbox(&mut w.edges_only, "Only where it reaches the edges of the picture");
            ui.label(egui::RichText::new("Colours this close to it go too. The edges of what is left stay smooth.").small());
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if apply {
        let params = json!({ "color": crate::from_color32(w.colour).to_hex(), "tolerance": w.tolerance });
        let id = if w.edges_only { "image.removeSolidBackground" } else { "image.makeColorTransparent" };
        if ed.run(ctx, id, params).is_some() {
            return;
        }
    }
    if open && !close && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ed.background.colour = Some(w);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;
    use screengrabcraft_model::{Document, EditorSettings};

    use super::*;
    use crate::services::{NoServices, Services};

    /// A model that calls the left half of a picture background, after `wait`; with `forever`, it
    /// works until it is stopped.
    struct Halves {
        wait: Duration,
        forever: bool,
    }

    impl Matting for Halves {
        fn name(&self) -> &str {
            "halves"
        }

        fn matte(&self, picture: &Raster, progress: &mut dyn FnMut(f32) -> bool) -> Result<Matte, MatteError> {
            let started = Instant::now();
            while self.forever || started.elapsed() < self.wait {
                if !progress((started.elapsed().as_secs_f32() / 5.0).min(0.99)) {
                    return Err(MatteError::Cancelled);
                }
                if started.elapsed() > Duration::from_secs(10) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let (w, h) = (picture.width(), picture.height());
            Ok(Matte::new(w, h, (0..h).flat_map(|_| (0..w).map(move |x| if x < w / 2 { 0 } else { 255 })).collect())?)
        }
    }

    struct WithModel(Arc<dyn Matting>);

    impl Services for WithModel {
        fn background_model(&self) -> Result<Arc<dyn Matting>, String> {
            Ok(self.0.clone())
        }
    }

    fn harness(model: Halves) -> Harness<'static, Editor> {
        let mut ed = Editor::new(Box::new(WithModel(Arc::new(model))), EditorSettings::default());
        ed.session.replace(Document::new(Raster::filled(200, 100, Color::WHITE).unwrap()));
        Harness::builder().with_size(egui::vec2(1000.0, 700.0)).build_ui_state(
            |ui, ed: &mut Editor| {
                let ctx = ui.ctx().clone();
                ed.logic(&ctx);
                ed.ui(ui);
            },
            ed,
        )
    }

    fn start_it(h: &mut Harness<'_, Editor>) {
        let ctx = h.ctx.clone();
        let model = h.state().services.background_model().unwrap();
        start(h.state_mut(), &ctx, model);
    }

    /// Runs frames until the model has answered (the window keeps drawing meanwhile).
    fn until_done(h: &mut Harness<'_, Editor>) {
        let started = Instant::now();
        while h.state().background.working() && started.elapsed() < Duration::from_secs(20) {
            h.run_steps(1);
            std::thread::sleep(Duration::from_millis(5));
        }
        h.run_steps(2);
    }

    #[test]
    fn remove_background_works_beside_the_window_and_is_one_undo_step() {
        let mut h = harness(Halves { wait: Duration::from_millis(300), forever: false });
        h.run_steps(2);
        start_it(&mut h);
        h.run_steps(2);
        assert!(h.state().background.working(), "still at work while the window draws");
        h.get_by_label("Removing the background");
        until_done(&mut h);
        let ed = h.state();
        assert!(!ed.background.working());
        assert_eq!(ed.session.doc.base.pixel(10, 50).map(|c| c.a), Some(0));
        assert_eq!(ed.session.doc.base.pixel(190, 50).map(|c| c.a), Some(255));
        assert_eq!(ed.session.undo_label(), Some("Remove Background"));
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.doc.base.pixel(10, 50).map(|c| c.a), Some(255));
    }

    #[test]
    fn stop_leaves_the_picture_as_it_was() {
        let mut h = harness(Halves { wait: Duration::ZERO, forever: true });
        h.run_steps(2);
        start_it(&mut h);
        h.run_steps(3);
        h.get_by_label("Stop").click();
        until_done(&mut h);
        let ed = h.state();
        assert!(!ed.background.working());
        assert!(!ed.session.can_undo());
        assert_eq!(ed.session.doc.base.pixel(10, 50), Some(Color::WHITE));
    }

    #[test]
    fn a_picture_changed_meanwhile_is_left_alone() {
        let mut h = harness(Halves { wait: Duration::from_millis(200), forever: false });
        h.run_steps(2);
        start_it(&mut h);
        let ctx = h.ctx.clone();
        h.state_mut().run(&ctx, "effect.invert", json!({}));
        until_done(&mut h);
        let ed = h.state();
        assert_eq!(ed.session.doc.base.pixel(10, 50), Some(Color::BLACK), "inverted, and nothing more");
        assert_eq!(ed.session.undo_label(), Some("Invert Colors"));
    }

    /// Pictures of the menu, the progress window and the colour window, drawn with the GPU, for
    /// looking at: `SCREENGRABCRAFT_SHOTS=<dir> cargo test -p screengrabcraft-ui -- --ignored pictures_of`.
    #[test]
    #[ignore = "draws with the GPU; writes pictures to SCREENGRABCRAFT_SHOTS"]
    fn pictures_of_the_background_windows() {
        let dir = std::path::PathBuf::from(std::env::var("SCREENGRABCRAFT_SHOTS").unwrap_or_else(|_| ".".to_string()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut h = harness(Halves { wait: Duration::ZERO, forever: true });
        h.run_steps(3);
        h.get_by_label("Image").click();
        h.run_steps(3);
        h.get_by_label("Remove Background").hover();
        h.run_steps(30);
        h.render().unwrap().save(dir.join("image-menu.png")).unwrap();
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        start_it(&mut h);
        std::thread::sleep(Duration::from_millis(1500));
        h.run_steps(3);
        h.render().unwrap().save(dir.join("removing-the-background.png")).unwrap();
        h.get_by_label("Stop").click();
        until_done(&mut h);
        let ed = h.state_mut();
        ed.background.colour = Some(ColourWindow::new(&ed.session.doc.base));
        h.run_steps(3);
        h.render().unwrap().save(dir.join("make-a-colour-transparent.png")).unwrap();
    }

    #[test]
    fn without_a_model_it_says_why() {
        let why = NoServices.background_model().err().unwrap_or_default();
        assert!(why.contains("browser"), "{why}");
    }

    #[test]
    fn the_colour_window_makes_a_colour_transparent() {
        let mut h = harness(Halves { wait: Duration::ZERO, forever: false });
        {
            let ed = h.state_mut();
            let mut r = Raster::filled(200, 100, Color::WHITE).unwrap();
            r.pixels_mut()[..4].copy_from_slice(&[0, 0, 0, 255]);
            ed.session.replace(Document::new(r));
            ed.background.colour = Some(ColourWindow::new(&ed.session.doc.base));
        }
        h.run_steps(2);
        h.get_by_label("Apply").click();
        h.run_steps(3);
        let ed = h.state();
        assert!(ed.background.colour.is_none(), "applied and closed");
        assert_eq!(ed.session.doc.base.pixel(100, 50).map(|c| c.a), Some(0));
        assert_eq!(ed.session.doc.base.pixel(0, 0).map(|c| c.a), Some(255), "the black pixel stays");
        assert_eq!(ed.session.undo_label(), Some("Make a Colour Transparent"));
    }
}
