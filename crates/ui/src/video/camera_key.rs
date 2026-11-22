//! The camera's background taken away, in the camera's panel: a colour (a green screen), or the
//! background itself, learnt by the background model from a few of the camera's pictures (see
//! `screengrabcraft_media::keyer`). Learning takes seconds a picture, so it works on a thread of
//! its own while the panel says how far it has got; what it learns is added to the recording
//! file, so it's there the next time it's opened.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, channel};

use egui::{RichText, Ui};
use screengrabcraft_media::keyer::{Plate, Remove};
use screengrabcraft_media::recording::StreamKind;
use serde_json::json;

use super::{VideoView, named_slider};
use crate::editor::Editor;
use crate::theme::Tokens;

/// The background model at work on the camera's pictures.
pub(crate) struct Learning {
    /// How far it has got (0 to 1, as `f32` bits).
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
    answer: Receiver<Result<Plate, String>>,
}

impl Drop for Learning {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl VideoView {
    /// Starts learning the camera's background (the model is the host's; where there is none, it
    /// says why).
    pub(crate) fn learn_background(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        if self.learning.is_some() {
            return;
        }
        let Some(stream) = self.session.timeline.find(StreamKind::Camera).and_then(|i| self.session.timeline.tracks.get(i)).map(|t| t.stream) else {
            return;
        };
        let model = match ed.services.background_model() {
            Ok(m) => m,
            Err(e) => {
                ed.error(ctx, e);
                return;
            }
        };
        let progress = Arc::new(AtomicU32::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, answer) = channel();
        let (seen, stop, wake, rec) = (progress.clone(), cancel.clone(), ctx.clone(), self.rec.clone());
        let spawned = std::thread::Builder::new().name("camera-background".to_string()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                screengrabcraft_media::keyer::learn_from(&rec, stream, model.as_ref(), &mut |f| {
                    let f = if f.is_finite() { f.clamp(0.0, 1.0) } else { 0.0 };
                    seen.store(f.to_bits(), Ordering::Relaxed);
                    wake.request_repaint();
                    !stop.load(Ordering::Relaxed)
                })
            }))
            .unwrap_or_else(|_| Err("the background model failed unexpectedly".to_string()));
            let _ = tx.send(result);
            wake.request_repaint();
        });
        match spawned {
            Ok(_) => self.learning = Some(Learning { progress, cancel, answer }),
            Err(e) => ed.error(ctx, format!("The background couldn't be learnt: {e}")),
        }
    }

    /// Every frame: the background learnt, once it is, kept with the recording (and in its file).
    pub(super) fn poll_learning(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let Some(job) = &self.learning else { return };
        let Ok(answer) = job.answer.try_recv() else { return };
        self.learning = None;
        match answer {
            Ok(plate) => {
                if let Some(path) = &self.path {
                    let saved = screengrabcraft_media::recording::plate_bytes(&plate)
                        .map_err(|e| e.to_string())
                        .and_then(|b| ed.services.append_file(path, &b));
                    if let Err(e) = saved {
                        log::warn!("the camera's background couldn't be kept in the recording: {e}");
                    }
                }
                self.rec.set_plate(plate);
                self.shown = None;
                ed.toast(ctx, "The camera's background is learnt: the screen shows round you");
            }
            Err(e) => ed.error(ctx, format!("The camera's background couldn't be learnt: {e}")),
        }
    }

    /// What shows where the camera's background is taken away: the screen, the camera's own
    /// picture blurred, or a picture of the media bin.
    fn camera_behind(&mut self, ed: &mut Editor, ui: &mut Ui) {
        use screengrabcraft_media::layers::MediaKind;
        use screengrabcraft_media::timeline::Behind;
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let behind = self.session.timeline.camera.behind;
        let pictures: Vec<(u32, String)> =
            self.session.timeline.media.iter().filter(|m| m.kind == MediaKind::Picture).map(|m| (m.id, m.name.clone())).collect();
        let name = |b: Behind| match b {
            Behind::Screen => "The screen".to_string(),
            Behind::Blur => "Your room, blurred".to_string(),
            Behind::Picture(id) => pictures.iter().find(|(m, _)| *m == id).map_or("A picture".to_string(), |(_, n)| n.clone()),
        };
        ui.add_space(6.0);
        ui.label(RichText::new("Behind you").size(12.0).color(t.muted));
        let mut chosen = None;
        egui::ComboBox::from_id_salt("camera-behind").width(ui.available_width()).selected_text(name(behind)).show_ui(ui, |ui| {
            for b in [Behind::Screen, Behind::Blur] {
                if ui.selectable_label(behind == b, name(b)).clicked() {
                    chosen = Some(json!({"behind": if b == Behind::Blur { "blur" } else { "screen" }}));
                }
            }
            for (id, n) in &pictures {
                if ui.selectable_label(behind == Behind::Picture(*id), n).on_hover_text("A picture of the Media bin").clicked() {
                    chosen = Some(json!({"behind": "picture", "picture": id}));
                }
            }
        });
        if pictures.is_empty() {
            ui.label(RichText::new("Pictures brought into the Media bin can go behind you too.").size(12.0).color(t.muted));
        }
        if let Some(p) = chosen {
            self.run(ed, &ctx, "video.cameraKey", p);
        }
    }

    /// What's taken away from the camera's picture: nothing, a colour (and which, how far from
    /// it), or the background (learnt, or being learnt).
    pub(super) fn camera_background(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let cam = self.session.timeline.camera;
        let small = |s: &str| RichText::new(s).size(12.0).color(t.muted);
        ui.label(small("Background"));
        let label = |r: Remove| match r {
            Remove::Nothing => "Keep it",
            Remove::Colour => "Remove a colour (green screen)",
            Remove::Background => "Remove it (no green screen)",
        };
        let mut remove = cam.remove;
        let model = ed.services.background_model();
        egui::ComboBox::from_id_salt("camera-background").width(ui.available_width()).selected_text(label(remove)).show_ui(ui, |ui| {
            for r in [Remove::Nothing, Remove::Colour, Remove::Background] {
                let can = r != Remove::Background || model.is_ok();
                let why = model.as_ref().err().cloned().unwrap_or_default();
                ui.add_enabled_ui(can, |ui| ui.selectable_value(&mut remove, r, label(r))).inner.on_disabled_hover_text(why);
            }
        });
        if remove != cam.remove {
            self.run(ed, &ctx, "video.cameraKey", json!({"remove": remove}));
            if remove == Remove::Background && self.rec.plate().is_none() {
                self.learn_background(ed, &ctx);
            }
        }
        if cam.remove != Remove::Nothing {
            self.camera_behind(ed, ui);
        }
        match cam.remove {
            Remove::Nothing => {}
            Remove::Colour => {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let mut c = cam.key_colour;
                    if egui::widgets::color_picker::color_edit_button_srgb(ui, &mut c).on_hover_text("The colour taken away").changed() {
                        let hex = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
                        self.run(ed, &ctx, "video.cameraKey", json!({"colour": hex}));
                    }
                    ui.label(small("The colour taken away"));
                });
                ui.add_space(6.0);
                let mut tol = cam.key_tolerance * 100.0;
                let r = named_slider(ui, "Tolerance", &mut tol, 0.0..=100.0, "%").on_hover_text("How far from the colour a colour still goes");
                if r.drag_started() {
                    self.session.begin("Camera Background");
                }
                if r.changed() {
                    let p = json!({"tolerance": tol / 100.0});
                    if r.dragged() {
                        let _ = self.session.drag("video.cameraKey", &p);
                    } else {
                        self.run(ed, &ctx, "video.cameraKey", p);
                    }
                }
                if r.drag_stopped() {
                    self.session.commit();
                }
            }
            Remove::Background => {
                ui.add_space(6.0);
                if let Some(job) = &self.learning {
                    let p = f32::from_bits(job.progress.load(Ordering::Relaxed));
                    ui.label(small("Learning the background from a few of the camera's pictures, on this computer…"));
                    ui.add(egui::ProgressBar::new(p).show_percentage().animate(true));
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                } else if self.rec.plate().is_some() {
                    ui.label(small("Learnt from a few of the camera's pictures: keep the camera still. Learn it again if the light changed."));
                    if ui.button("Learn It Again").clicked() {
                        self.learn_background(ed, &ctx);
                    }
                } else {
                    ui.label(small("Not learnt yet."));
                    if ui.add_enabled(model.is_ok(), egui::Button::new("Learn the Background")).clicked() {
                        self.learn_background(ed, &ctx);
                    }
                }
            }
        }
    }
}
