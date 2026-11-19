//! The Effects pane, docked under Properties at the right: the picture's effects as a list that
//! is always there, each a section to open with its settings and Apply (border, drop shadow,
//! torn edge, round corners, brightness and contrast), and the colour changes that need none
//! (grayscale, invert). The Effects menu still opens each as a small window.

use egui::Ui;
use serde_json::{Value, json};

use crate::dialogs::{self, Dialog};
use crate::editor::Editor;
use crate::theme::Tokens;

/// Each effect's settings in the pane, kept while the editor is open (so a second Apply uses
/// what was set for the first).
#[derive(Clone, Debug)]
pub struct Effects {
    sections: Vec<Dialog>,
}

impl Default for Effects {
    fn default() -> Effects {
        Effects { sections: vec![Dialog::border(), Dialog::shadow(), Dialog::torn(), Dialog::round(), Dialog::brightness()] }
    }
}

/// What the colour section is called.
pub const COLOURS: &str = "Colours";

pub fn pane(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    // The settings' sliders as wide as the pane leaves room for beside their names and numbers.
    ui.spacing_mut().slider_width = (ui.available_width() - 170.0).clamp(60.0, 260.0);
    ui.label(egui::RichText::new("For the whole picture. Open one to set it, then Apply.").size(12.0).color(t.muted));
    ui.add_space(6.0);
    let mut apply: Option<(&'static str, Value)> = None;
    for d in &mut ed.effects.sections {
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
    }
    egui::CollapsingHeader::new(egui::RichText::new(COLOURS).strong()).id_salt(("effect", COLOURS)).show(ui, |ui| {
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Grayscale").on_hover_text("Turn the picture grey").clicked() {
                apply = Some(("effect.grayscale", json!({})));
            }
            if ui.button("Invert Colors").on_hover_text("Turn every colour into its opposite").clicked() {
                apply = Some(("effect.invert", json!({})));
            }
        });
        ui.add_space(4.0);
    });
    if let Some((id, params)) = apply
        && ed.run(&ctx, id, params).is_some()
    {
        // A border, a shadow or a torn edge changes the picture's size.
        ed.view.fit_pending = true;
    }
}

fn apply_button(ui: &mut Ui, t: &Tokens, label: &str) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new(label).color(t.accent_text)).fill(t.accent).min_size(egui::vec2(0.0, 28.0)))
}
