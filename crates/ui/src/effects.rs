//! The Effects pane, docked under Properties at the right: the picture's effects as a list that
//! is always there, each a section to open with its settings and Apply, and the colour changes
//! that need none (grayscale, sepia, invert). The colour changes, sharpen and blur keep to the
//! area selected on the picture when there is one. The Effects menu still opens each as a small
//! window.

use egui::Ui;
use serde_json::{Value, json};

use screengrabcraft_model::presets::{self, EffectPreset, EffectStep};

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
    /// The effect whose settings are being kept as a preset (its title), and the name typed.
    keeping: Option<(&'static str, String)>,
    /// Why the last preset couldn't be kept.
    problem: Option<String>,
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
            keeping: None,
            problem: None,
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
/// What the presets' section, the button keeping an effect's settings and the name box are called.
pub const PRESETS: &str = "Presets";
pub const KEEP_PRESET: &str = "Keep as Preset…";
pub const PRESET_NAME: &str = "Preset name";

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
    presets(ed, ui);
    let mut keep: Option<(String, &'static str, Value)> = None;
    for section in &mut ed.effects.sections {
        match section {
            Section::Effect(d) => {
                let title = d.title();
                egui::CollapsingHeader::new(egui::RichText::new(title).strong()).id_salt(("effect", title)).show(ui, |ui| {
                    ui.add_space(2.0);
                    let cmd = dialogs::body(d, ui);
                    ui.add_space(6.0);
                    if let Some(cmd) = cmd {
                        if apply_button(ui, &t, &format!("Apply {title}")).clicked() {
                            apply = Some(cmd.clone());
                        }
                        ui.add_space(4.0);
                        if let Some(name) = keep_row(ui, &t, &mut ed.effects.keeping, &mut ed.effects.problem, title) {
                            keep = Some((name, cmd.0, cmd.1));
                        }
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
    if let Some((name, id, params)) = keep {
        match ed.settings.keep_preset(&name, vec![EffectStep { effect: id.to_string(), params }]) {
            Ok(_) => {
                ed.effects.keeping = None;
                ed.effects.problem = None;
                ed.services.save_settings(&ed.settings);
            }
            Err(why) => ed.effects.problem = Some(why),
        }
    }
    if let Some((id, params)) = apply {
        ed.apply_effect(&ctx, id, params);
    }
}

/// Under an effect's Apply: Keep as Preset…, or, once clicked, the name to keep it under with
/// Keep and Cancel. The name, when Keep is clicked.
fn keep_row(
    ui: &mut Ui,
    t: &Tokens,
    keeping: &mut Option<(&'static str, String)>,
    problem: &mut Option<String>,
    title: &'static str,
) -> Option<String> {
    let mut kept = None;
    match keeping {
        Some((which, name)) if *which == title => {
            let field = egui::TextEdit::singleline(name).hint_text(PRESET_NAME).char_limit(presets::MAX_PRESET_NAME).desired_width(f32::INFINITY);
            ui.add(field).widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, PRESET_NAME));
            if let Some(p) = problem.as_ref() {
                ui.label(egui::RichText::new(p.as_str()).size(12.0).color(t.danger));
            }
            let mut cancel = false;
            ui.horizontal(|ui| {
                if ui.button("Keep").on_hover_text("Keep these settings under this name, with the presets").clicked() {
                    kept = Some(name.clone());
                }
                cancel = ui.button("Cancel").clicked();
            });
            if cancel {
                *keeping = None;
                *problem = None;
            }
        }
        _ => {
            if ui.small_button(KEEP_PRESET).on_hover_text("Keep these settings under a name, to apply with one click").clicked() {
                *keeping = Some((title, title.to_string()));
                *problem = None;
            }
        }
    }
    kept
}

/// What a preset does, for its tooltip: its effects' names, in order.
fn describe(p: &EffectPreset) -> String {
    let names: Vec<&str> = p.steps.iter().map(|s| screengrabcraft_engine::registry::find(&s.effect).map_or(s.effect.as_str(), |c| c.label)).collect();
    names.join(", then ")
}

/// The presets: the ready-made ones, then the owner's own (a right-click deletes one), each
/// applied with a click.
fn presets(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let own = ed.settings.effect_presets.clone();
    let mut apply = None;
    let mut forget = None;
    egui::CollapsingHeader::new(egui::RichText::new(PRESETS).strong()).id_salt(("effect", PRESETS)).show(ui, |ui| {
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            for p in presets::ready_made() {
                if ui.button(p.name.as_str()).on_hover_text(describe(&p)).clicked() {
                    apply = Some(p);
                }
            }
        });
        if own.is_empty() {
            ui.label(egui::RichText::new("Your own: open an effect below, set it, then Keep as Preset.").size(12.0).color(t.muted));
        } else {
            ui.add_space(2.0);
            ui.label(egui::RichText::new("Your own").size(12.0).color(t.muted));
            ui.horizontal_wrapped(|ui| {
                for p in &own {
                    let resp = ui.button(p.name.as_str()).on_hover_text(format!("{} (right-click to delete)", describe(p)));
                    if resp.clicked() {
                        apply = Some(p.clone());
                    }
                    resp.context_menu(|ui| {
                        if ui.button(format!("Delete {}", p.name)).clicked() {
                            forget = Some(p.name.clone());
                            ui.close();
                        }
                    });
                }
            });
        }
        ui.add_space(4.0);
    });
    if let Some(p) = apply {
        ed.apply_preset(&ctx, &p);
    }
    if let Some(name) = forget {
        ed.settings.forget_preset(&name);
        ed.services.save_settings(&ed.settings);
    }
}

fn apply_button(ui: &mut Ui, t: &Tokens, label: &str) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new(label).color(t.accent_text)).fill(t.accent).min_size(egui::vec2(0.0, 28.0)))
}
