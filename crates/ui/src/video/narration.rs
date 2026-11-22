//! Narration recorded over the video: the side panel's Narration section turns the chosen
//! microphone on (the host's, only when Record Narration is pressed), plays the video from the
//! playhead without its sound while the owner speaks, and puts the take on a sound layer of its
//! own where the playhead was.

use egui::{RichText, Ui, vec2};
use serde_json::json;

use super::{Playing, VideoView};
use crate::editor::Editor;
use crate::services::{NO_NARRATION, NarrationTake};
use crate::theme::Tokens;

/// The longest take (seconds): it stops by itself there.
const MOST_SECS: f64 = 3600.0;

/// A take being recorded: the microphone on, where the playhead was, and since when.
pub struct Narrating {
    pub take: Box<dyn NarrationTake>,
    pub at: u64,
    pub since: f64,
}

/// The microphones the host has (asked for when the list is first opened: listing them turns
/// none on), and the one chosen (none: the one Video Production records with).
#[derive(Clone, Debug, Default)]
pub struct Mics {
    pub names: Option<Vec<String>>,
    pub chosen: Option<String>,
}

/// What the microphone list says for none chosen.
const AS_RECORDED: &str = "As Video Production records";

impl VideoView {
    /// Whether a take is being recorded.
    pub fn narrating(&self) -> bool {
        self.narrating.is_some()
    }

    /// Turns the microphone on and plays the video from the playhead without its sound.
    pub fn start_narration(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        if self.narrating.is_some() {
            return;
        }
        let Some(path) = self.path.clone() else {
            return ed.error(ctx, "Narration is written beside the recording: this one isn't in a file.".to_string());
        };
        let now = ctx.input(|i| i.time);
        self.pause();
        if self.playhead >= self.length() {
            self.playhead = 0;
        }
        let mic = self.mics.chosen.clone();
        match ed.services.record_narration(mic.as_deref(), &path) {
            Ok(take) => {
                self.narrating = Some(Narrating { take, at: self.playhead, since: now });
                // The video's own sound isn't played: the microphone hears just the voice.
                self.playing = Some(Playing { since: now, from: self.playhead, rate: 1, in_range: false, sound: None });
            }
            Err(e) => ed.error(ctx, format!("The microphone didn't start: {e}")),
        }
    }

    /// Turns the microphone off; the take goes on a sound layer of its own where it began.
    pub fn stop_narration(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let Some(n) = self.narrating.take() else { return };
        self.pause();
        match n.take.finish() {
            Ok((path, duration)) => {
                let name = crate::editor::file_name(&path);
                let p = json!({"path": path, "name": name, "duration": duration, "at": n.at});
                match self.session.execute("video.addNarration", &p) {
                    Ok(_) => {
                        self.item = None;
                        ed.toast(ctx, format!("Your narration, {:.1} s, is on a sound layer of its own", duration as f64 / 1e6));
                    }
                    Err(e) => ed.error(ctx, e.to_string()),
                }
            }
            Err(e) => ed.error(ctx, format!("No narration was kept: {e}")),
        }
    }

    /// While a take is recorded: it stops with the video (or after an hour).
    pub(super) fn tick_narration(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let Some(n) = &self.narrating else { return };
        let now = ctx.input(|i| i.time);
        if self.playing.is_none() || now - n.since > MOST_SECS {
            self.stop_narration(ed, ctx);
        } else {
            ctx.request_repaint();
        }
    }

    /// The side panel's Narration section: the microphone, and Record Narration (or Stop, with how
    /// loud it is and how long so far).
    pub(super) fn narration_section(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        let host = ed.services.can_narrate();
        let can = host && self.path.is_some();
        if let Some(n) = &self.narrating {
            let secs = (ctx.input(|i| i.time) - n.since).max(0.0);
            let level = n.take.level().clamp(0.0, 1.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("\u{25cf} Recording {}", super::clock((secs * 1e6) as u64)))
                        .color(egui::Color32::from_rgb(0xE5, 0x48, 0x4D)),
                );
                let (r, _) = ui.allocate_exact_size(vec2(90.0, 8.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 3.0, t.control);
                ui.painter().rect_filled(egui::Rect::from_min_size(r.min, vec2(r.width() * level, r.height())), 3.0, t.accent);
            });
            ui.add_space(4.0);
            if crate::widgets::primary_button(ui, "Stop Narration").on_hover_text("Turn the microphone off and put the take on the video").clicked() {
                self.stop_narration(ed, &ctx);
            }
            return;
        }
        let shown = self.mics.chosen.clone().unwrap_or_else(|| AS_RECORDED.to_string());
        ui.add_enabled_ui(can, |ui| {
            ui.label(RichText::new("Record with").size(12.0).color(t.muted));
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("narration-mic").width((ui.available_width() - 12.0).clamp(120.0, 240.0)).selected_text(shown).show_ui(
                    ui,
                    |ui| {
                        // Listed when the list is first opened.
                        let names = self.mics.names.get_or_insert_with(|| ed.services.microphones()).clone();
                        ui.selectable_value(&mut self.mics.chosen, None, AS_RECORDED);
                        for name in names {
                            ui.selectable_value(&mut self.mics.chosen, Some(name.clone()), name);
                        }
                    },
                );
            });
        });
        ui.add_space(4.0);
        if ui
            .add_enabled(can, egui::Button::new("Record Narration"))
            .on_hover_text("Turns the microphone on and plays the video from the playhead without its sound while you speak")
            .on_disabled_hover_text(if host { "Narration is written beside the recording: this one isn't in a file" } else { NO_NARRATION })
            .clicked()
        {
            self.start_narration(ed, &ctx);
        }
        if self.mics.names.as_ref().is_some_and(Vec::is_empty) {
            ui.label(RichText::new("No microphone was found.").size(12.0).color(t.muted));
        }
        ui.label(
            RichText::new("Your voice goes on a sound layer of its own from where the playhead was. Pause, or Stop, to end it.")
                .size(12.0)
                .color(t.muted),
        );
    }
}
