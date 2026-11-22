//! The Effects pane, docked under Properties at the right: the picture's effects as a list that
//! is always there, each a section to open with its settings and Apply, and the colour changes
//! that need none (grayscale, sepia, invert). The colour changes, sharpen and blur keep to the
//! area selected on the picture when there is one. The Effects menu still opens each as a small
//! window.

use egui::Ui;
use serde_json::{Value, json};

use crate::dialogs::{self, Dialog};
use crate::editor::Editor;
use crate::theme::Tokens;

/// One of the pane's sections.
#[derive(Clone, Debug)]
enum Section {
    Effect(Dialog),
    /// Grayscale, sepia and invert: a click each.
    Colours,
}

/// Each effect's settings in the pane, kept while the editor is open (so a second Apply uses
/// what was set for the first).
#[derive(Clone, Debug)]
pub struct Effects {
    sections: Vec<Section>,
}

impl Default for Effects {
    fn default() -> Effects {
        let e = Section::Effect;
        Effects {
            sections: vec![
                // The colours first, then the edges and the shape, then what is written on it.
                e(Dialog::brightness()),
                e(Dialog::hue()),
                Section::Colours,
                e(Dialog::sharpen()),
                e(Dialog::blur()),
                e(Dialog::border()),
                e(Dialog::shadow()),
                e(Dialog::round()),
                e(Dialog::torn()),
                e(Dialog::wavy()),
                e(Dialog::faded()),
                e(Dialog::bevel()),
                e(Dialog::perspective()),
                e(Dialog::curl()),
                e(Dialog::reflection()),
                e(Dialog::watermark()),
                e(Dialog::details()),
            ],
        }
    }
}

impl Effects {
    /// The watermark's picture was chosen: its section says which.
    pub(crate) fn watermark_chosen(&mut self, name: &str) {
        for s in &mut self.sections {
            if let Section::Effect(Dialog::Watermark { picture, words, .. }) = s {
                *picture = Some(name.to_string());
                *words = false;
            }
        }
    }
}

/// What the colour section is called.
pub const COLOURS: &str = "Colours";

pub fn pane(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    // The settings' sliders as wide as the pane leaves room for beside their names and numbers.
    ui.spacing_mut().slider_width = (ui.available_width() - 170.0).clamp(60.0, 260.0);
    let how = if ed.area.is_some() {
        "The colour changes, Sharpen and Blur are for the selected area; the rest for the whole picture."
    } else {
        "For the whole picture, or the colour changes for an area selected on it. Open one to set it, then Apply."
    };
    ui.label(egui::RichText::new(how).size(12.0).color(t.muted));
    ui.add_space(6.0);
    let mut apply: Option<(&'static str, Value)> = None;
    let mut choose = false;
    for section in &mut ed.effects.sections {
        match section {
            Section::Effect(d) => {
                let title = d.title();
                egui::CollapsingHeader::new(egui::RichText::new(title).strong()).id_salt(("effect", title)).show(ui, |ui| {
                    ui.add_space(2.0);
                    let cmd = dialogs::body(d, ui);
                    ui.add_space(6.0);
                    if let Some(cmd) = cmd
                        && apply_button(ui, &t, &format!("Apply {title}")).clicked()
                    {
                        apply = Some(cmd);
                    }
                    ui.add_space(4.0);
                });
                if let Dialog::Watermark { choose: c, .. } = d {
                    choose |= std::mem::take(c);
                }
            }
            Section::Colours => {
                egui::CollapsingHeader::new(egui::RichText::new(COLOURS).strong()).id_salt(("effect", COLOURS)).show(ui, |ui| {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Grayscale").on_hover_text("Turn the picture grey").clicked() {
                            apply = Some(("effect.grayscale", json!({})));
                        }
                        if ui.button("Sepia").on_hover_text("The browns of an old photograph").clicked() {
                            apply = Some(("effect.sepia", json!({})));
                        }
                        if ui.button("Invert Colors").on_hover_text("Turn every colour into its opposite").clicked() {
                            apply = Some(("effect.invert", json!({})));
                        }
                    });
                    ui.add_space(4.0);
                });
            }
        }
    }
    if choose {
        ed.choose_watermark();
    }
    if let Some((id, params)) = apply {
        ed.apply_effect(&ctx, id, params);
    }
}

fn apply_button(ui: &mut Ui, t: &Tokens, label: &str) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new(label).color(t.accent_text)).fill(t.accent).min_size(egui::vec2(0.0, 28.0)))
}
