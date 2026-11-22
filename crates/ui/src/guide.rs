//! The how-to guide window: the steps a step capture took, each its number, its picture (the
//! click marked on it) and its words, to put in order, take out and reword; then saved as a PDF,
//! a web page or Markdown. Every change is a guide command (`guide.*`); the host (the capture
//! service) reads and writes the files.

use std::collections::HashMap;

use egui::{Color32, Ui, vec2};
use screengrabcraft_model::Raster;
use screengrabcraft_model::guide::Guide;
use serde_json::{Value, json};

use crate::theme::Tokens;
use crate::widgets;

/// What a guide is saved as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideFormat {
    Pdf,
    Html,
    Markdown,
}

impl GuideFormat {
    pub const ALL: [GuideFormat; 3] = [GuideFormat::Pdf, GuideFormat::Html, GuideFormat::Markdown];

    pub fn label(self) -> &'static str {
        match self {
            GuideFormat::Pdf => "Save as PDF",
            GuideFormat::Html => "Save as Web Page",
            GuideFormat::Markdown => "Save as Markdown",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            GuideFormat::Pdf => "pdf",
            GuideFormat::Html => "html",
            GuideFormat::Markdown => "md",
        }
    }
}

/// What the guide window asks of its host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideAction {
    /// The guide changed: write its file.
    Save,
    Export(GuideFormat),
    /// Show the guide's file in its folder.
    ShowFolder,
    Close,
}

/// The thumbnails' width.
const THUMB_W: f32 = 260.0;

/// A guide being edited.
pub struct GuideView {
    pub guide: Guide,
    /// The steps' pictures, by file name (none: it couldn't be read).
    pictures: HashMap<String, Option<Raster>>,
    textures: HashMap<String, egui::TextureHandle>,
    /// What the last export said.
    pub message: Option<(String, bool)>,
}

impl GuideView {
    pub fn new(guide: Guide, pictures: HashMap<String, Option<Raster>>) -> GuideView {
        GuideView { guide, pictures, textures: HashMap::new(), message: None }
    }

    /// A step's picture.
    pub fn picture(&self, name: &str) -> Option<&Raster> {
        self.pictures.get(name).and_then(Option::as_ref)
    }

    /// Runs a guide command; false (and why, in the log) if it was refused.
    pub fn run(&mut self, id: &str, params: Value) -> bool {
        match screengrabcraft_engine::guide::run(&mut self.guide, id, &params) {
            Ok(_) => true,
            Err(e) => {
                log::warn!("{id}: {e}");
                false
            }
        }
    }

    fn texture(&mut self, ctx: &egui::Context, name: &str) -> Option<egui::TextureHandle> {
        if let Some(t) = self.textures.get(name) {
            return Some(t.clone());
        }
        let r = self.picture(name)?;
        let f = (THUMB_W * 2.0 / r.width().max(1) as f32).min(1.0);
        let small =
            screengrabcraft_render::effects::resize(r, ((r.width() as f32 * f) as u32).max(1), ((r.height() as f32 * f) as u32).max(1)).ok()?;
        let img = egui::ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.pixels());
        let t = ctx.load_texture(format!("guide-{name}"), img, egui::TextureOptions::LINEAR);
        self.textures.insert(name.to_string(), t.clone());
        Some(t)
    }
}

/// The guide window's contents. What was asked of the host, if anything.
pub fn show(view: &mut GuideView, ui: &mut Ui) -> Option<GuideAction> {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let mut action = None;
    egui::Panel::top("guide-bar").frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(egui::Margin::symmetric(16, 10))).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Title");
            let mut title = view.guide.title.clone();
            if ui.add(egui::TextEdit::singleline(&mut title).hint_text("How to…").desired_width(320.0)).changed()
                && view.run("guide.title", json!({"title": title}))
            {
                action = Some(GuideAction::Save);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Show in Folder").clicked() {
                    action = Some(GuideAction::ShowFolder);
                }
                let any = !view.guide.steps.is_empty();
                for f in GuideFormat::ALL.into_iter().rev() {
                    if ui.add_enabled(any, egui::Button::new(f.label())).clicked() {
                        action = Some(GuideAction::Export(f));
                    }
                }
            });
        });
        if let Some((words, error)) = &view.message {
            ui.label(egui::RichText::new(words).color(if *error { t.danger } else { t.muted }));
        }
    });
    egui::Panel::bottom("guide-footer").frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(egui::Margin::symmetric(16, 8))).show(ui, |ui| {
        ui.horizontal(|ui| {
            let n = view.guide.steps.len();
            ui.label(egui::RichText::new(format!("{n} step{}", if n == 1 { "" } else { "s" })).color(t.muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::button(ui, "Close").clicked() {
                    action = Some(GuideAction::Close);
                }
            });
        });
    });
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.window).inner_margin(16)).show(ui, |ui| {
        if view.guide.steps.is_empty() {
            ui.label(egui::RichText::new("No steps: every step was taken out, or nothing was clicked.").color(t.muted));
            return;
        }
        let mut change: Option<(&'static str, Value)> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let count = view.guide.steps.len();
            for i in 0..count {
                let Some(step) = view.guide.steps.get(i).cloned() else { continue };
                egui::Frame::NONE.fill(t.panel).stroke(egui::Stroke::new(1.0, t.border)).corner_radius(8).inner_margin(12).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        badge(ui, i + 1, t.accent);
                        match view.texture(&ctx, &step.picture) {
                            Some(tex) => {
                                let size = tex.size_vec2();
                                let f = THUMB_W / size.x.max(1.0);
                                ui.add(egui::Image::new(&tex).fit_to_exact_size(size * f)).on_hover_text(&step.picture);
                            }
                            None => {
                                ui.add_sized(vec2(THUMB_W, 60.0), egui::Label::new(egui::RichText::new("The picture is missing").color(t.danger)));
                            }
                        }
                        ui.vertical(|ui| {
                            let mut words = step.text.clone();
                            let edit = egui::TextEdit::multiline(&mut words)
                                .hint_text("What to do in this step")
                                .desired_rows(3)
                                .desired_width(f32::INFINITY);
                            if ui.add(edit).changed() {
                                change = Some(("guide.text", json!({"step": i, "text": words})));
                            }
                            ui.horizontal(|ui| {
                                if ui.add_enabled(i > 0, egui::Button::new("Move Up")).clicked() {
                                    change = Some(("guide.move", json!({"step": i, "to": i - 1})));
                                }
                                if ui.add_enabled(i + 1 < count, egui::Button::new("Move Down")).clicked() {
                                    change = Some(("guide.move", json!({"step": i, "to": i + 1})));
                                }
                                if ui.button(format!("Delete Step {}", i + 1)).clicked() {
                                    change = Some(("guide.delete", json!({"step": i})));
                                }
                            });
                        });
                    });
                });
                ui.add_space(10.0);
            }
        });
        if let Some((id, params)) = change
            && view.run(id, params)
        {
            action = Some(GuideAction::Save);
        }
    });
    action
}

/// A step's number in a round badge.
fn badge(ui: &mut Ui, n: usize, fill: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 16.0, fill);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, n.to_string(), egui::FontId::proportional(16.0), Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;
    use screengrabcraft_model::Color;
    use screengrabcraft_model::guide::GuideStep;

    use super::*;

    fn view() -> GuideView {
        let steps: Vec<GuideStep> =
            (1..=3).map(|i| GuideStep { picture: format!("step-{i:02}.png"), text: format!("Click thing {i}"), click: Some((10, 10)) }).collect();
        let mut pictures: HashMap<String, Option<Raster>> = HashMap::new();
        for (i, s) in steps.iter().enumerate() {
            let c = [Color::rgb(220, 60, 60), Color::rgb(60, 160, 60), Color::rgb(60, 90, 220)].get(i).copied().unwrap_or(Color::WHITE);
            pictures.insert(s.picture.clone(), Raster::filled(320, 200, c).ok());
        }
        pictures.insert("step-03.png".into(), None);
        GuideView::new(Guide { title: "Log in".into(), steps }, pictures)
    }

    type State = (GuideView, Vec<GuideAction>);

    fn harness() -> Harness<'static, State> {
        let mut h = Harness::builder().with_size(vec2(900.0, 760.0)).build_ui_state(
            |ui, s: &mut State| {
                crate::theme::apply(ui.ctx(), screengrabcraft_model::Theme::Light);
                if let Some(a) = show(&mut s.0, ui) {
                    s.1.push(a);
                }
            },
            (view(), Vec::new()),
        );
        h.run_steps(3);
        h
    }

    fn order(h: &Harness<'_, State>) -> Vec<String> {
        h.state().0.guide.steps.iter().map(|s| s.text.clone()).collect()
    }

    #[test]
    fn steps_are_reordered_taken_out_and_saved_as_a_pdf_a_web_page_or_markdown() {
        let mut h = harness();
        h.get_by_label("The picture is missing");
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/guide-window.png"));
        }
        if let Some(n) = h.get_all_by_label("Move Down").next() {
            n.click();
        }
        h.run_steps(3);
        assert_eq!(order(&h), ["Click thing 2", "Click thing 1", "Click thing 3"]);
        assert_eq!(h.state().1.last(), Some(&GuideAction::Save), "a change is saved");
        h.get_by_label("Delete Step 3").click();
        h.run_steps(3);
        assert_eq!(order(&h), ["Click thing 2", "Click thing 1"]);
        for f in GuideFormat::ALL {
            h.get_by_label(f.label()).click();
            h.run_steps(2);
            assert_eq!(h.state().1.last(), Some(&GuideAction::Export(f)));
        }
        h.get_by_label("Close").click();
        h.run_steps(2);
        assert_eq!(h.state().1.last(), Some(&GuideAction::Close));
    }

    #[test]
    fn the_words_of_a_step_are_changed_by_typing() {
        let mut h = harness();
        if let Some(first) = h.get_all_by_role(egui::accesskit::Role::MultilineTextInput).next() {
            first.click();
        }
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(1);
        h.event(egui::Event::Text("Open the menu".into()));
        h.run_steps(3);
        assert_eq!(h.state().1.last(), Some(&GuideAction::Save));
        assert_eq!(order(&h).first().map(String::as_str), Some("Open the menu"));
    }
}
