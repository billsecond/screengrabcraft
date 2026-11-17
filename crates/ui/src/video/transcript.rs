//! The transcript pane at the right of the video editor: everything said in the video as edited,
//! line by line, each with when it is and who said it (You on the microphone, Others on the
//! computer's sound, a layer's name for what was put on it). A click on a line takes the playhead
//! there; the line playing is marked, and followed while the video plays. Each speaker's name is
//! the name of the track (or the layer) it was heard on, and is renamed here or in its header.

use egui::{Color32, Rect, RichText, Sense, Ui, vec2};
use screengrabcraft_media::captions::{self, Cue, Speaker, Who};

use super::{Rename, VideoView, clock, kind_colour};
use crate::editor::Editor;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

/// The pane's width.
pub const TRANSCRIPT_W: f32 = 300.0;
/// The room round each line's words.
const PAD: f32 = 7.0;
/// A line's heading (when, who) above its words.
const HEAD: f32 = 18.0;

impl VideoView {
    /// The colour a speaker is marked with: its track's, or a sound layer's.
    fn who_colour(who: Who) -> Color32 {
        match who {
            Who::Speaker(s) => kind_colour(s.source()),
            Who::Layer(_) => Color32::from_rgb(0x2F, 0x80, 0xED),
        }
    }

    /// What renaming a speaker renames: the track it was heard on, or its layer.
    fn who_rename(&self, who: Who) -> Option<Rename> {
        match who {
            Who::Speaker(s) => self.session.timeline.find(s.source()).map(Rename::Track),
            Who::Layer(id) => self.session.timeline.layer(id).map(|_| Rename::Layer(id)),
        }
    }

    /// The pane: its heading (Copy, and the way to close it), who speaks (each name, to rename),
    /// then the lines.
    pub(super) fn transcript_panel(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        egui::Panel::right("video_transcript")
            .exact_size(TRANSCRIPT_W)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::same(14)))
            .show(ui, |ui| {
                let lines = self.lines.clone();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Transcript").size(16.0).color(t.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::button_sized(ui, Icon::Close, "Close the Transcript", false, true, 26.0).clicked() {
                            self.transcript_open = false;
                        }
                        if !lines.is_empty() && icons::button_sized(ui, Icon::Copy, "Copy the Whole Transcript", false, true, 26.0).clicked() {
                            ui.ctx().copy_text(captions::to_text_named(&self.named_lines()));
                            ed.toast(ui.ctx(), "The transcript is copied");
                        }
                    });
                });
                if lines.is_empty() {
                    self.transcript_empty(ed, ui);
                    return;
                }
                ui.add_space(6.0);
                ui.label(RichText::new("Speakers").size(12.0).color(t.muted));
                let mut speakers: Vec<Who> = Vec::new();
                for (_, w) in lines.iter() {
                    if !speakers.contains(w) && speakers.len() < 32 {
                        speakers.push(*w);
                    }
                }
                speakers.sort();
                for who in speakers {
                    self.speaker_row(ed, ui, who);
                }
                ui.add_space(6.0);
                ui.separator();
                self.transcript_lines(ed, ui, &lines);
            });
    }

    /// A speaker: their mark, their name (renamed with the pencil, or a double click), and where
    /// they were heard.
    fn speaker_row(&mut self, ed: &mut Editor, ui: &mut Ui, who: Who) {
        let t = Tokens::get(ui.ctx());
        let name = captions::speaker_name(&self.session.timeline, who);
        let target = self.who_rename(who);
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(vec2(12.0, 22.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.5, Self::who_colour(who));
            if let Some(target) = target
                && self.rename_box(ed, ui, target, None)
            {
                return;
            }
            let label = ui.add(egui::Label::new(RichText::new(&name).strong().color(t.text)).sense(Sense::click()));
            let heard = match who {
                Who::Speaker(Speaker::You) => "microphone",
                Who::Speaker(_) => "computer's sound",
                Who::Layer(_) => "layer",
            };
            ui.label(RichText::new(heard).size(11.5).color(t.muted));
            if let Some(target) = target {
                let renamed = label.on_hover_text("Double-click to rename").double_clicked();
                let pencil = ui
                    .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        icons::button_sized(ui, Icon::Edit, &format!("Rename {name}"), false, true, 22.0).clicked()
                    })
                    .inner;
                if renamed || pencil {
                    self.start_rename(target, &name, true);
                }
            }
        });
    }

    /// No lines: why, and the way to have some.
    fn transcript_empty(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        ui.add_space(12.0);
        let (words, offer) = if let Some((_, p)) = self.transcribing() {
            let done = p.map(|p| format!(" {:.0}%", p * 100.0)).unwrap_or_default();
            (format!("Writing down what's said, on this computer…{done} The lines appear here when they're ready."), false)
        } else if self.captions.is_some() || !self.media_cues.is_empty() {
            ("Nothing said is left in the video as edited: it was cut, muted or hidden where it was said.".to_string(), false)
        } else if self.can_transcribe() {
            (
                "Nothing has been written down yet. Transcribe writes down what's said in the video, on this computer, and who said it.".to_string(),
                true,
            )
        } else if self.hears_anything() {
            ("Transcripts are made of recordings saved in a file.".to_string(), false)
        } else {
            ("This video has nothing to hear.".to_string(), false)
        };
        ui.label(RichText::new(words).color(t.muted));
        if offer {
            ui.add_space(8.0);
            if crate::widgets::primary_button(ui, "Transcribe").clicked() {
                self.transcribe(ed, &ctx, false);
            }
        }
    }

    /// The lines, newest last: when, who and what, the one playing marked (and kept in view while
    /// the video plays); a click takes the playhead there. Each line is laid out once (for these
    /// lines, at this width), and only those in view are drawn.
    fn transcript_lines(&mut self, ed: &mut Editor, ui: &mut Ui, lines: &[(Cue, Who)]) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let width = ui.available_width().max(60.0);
        let words_w = (width - 2.0 * PAD).max(40.0);
        let words_font = egui::FontId::proportional(13.0);
        let mut heights = match self.transcript_heights.take() {
            Some((rev, w, h)) if rev == self.lines_rev && (w - width).abs() < 0.5 && h.len() == lines.len() => h,
            _ => lines
                .iter()
                .map(|(c, _)| PAD + HEAD + ui.painter().layout(c.text.clone(), words_font.clone(), t.text, words_w).size().y + PAD)
                .collect(),
        };
        heights.truncate(lines.len());
        let total: f32 = heights.iter().sum();
        let tl = &self.session.timeline;
        let mut names: Vec<(Who, String)> = Vec::new();
        for (_, w) in lines {
            if !names.iter().any(|(n, _)| n == w) && names.len() < 64 {
                names.push((*w, captions::speaker_name(tl, *w)));
            }
        }
        let now = lines.iter().position(|(c, _)| c.start <= self.playhead && self.playhead < c.end);
        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("transcript-lines");
        // Playing: the line playing is kept in view (once each time it changes).
        if self.playing.is_some() && now.is_some() && now != self.transcript_follow {
            let top: f32 = heights.iter().take(now.unwrap_or(0)).sum();
            scroll = scroll.vertical_scroll_offset((top - 90.0).max(0.0));
        }
        if self.playing.is_some() {
            self.transcript_follow = now;
        }
        let mut go = None;
        scroll.show_viewport(ui, |ui, view| {
            ui.set_height(total);
            let origin = ui.max_rect().min;
            let mut y = 0.0;
            for (k, ((c, who), h)) in lines.iter().zip(heights.iter()).enumerate() {
                let (top, bottom) = (y, y + h);
                y = bottom;
                if bottom < view.min.y {
                    continue;
                }
                if top > view.max.y {
                    break;
                }
                let rect = Rect::from_min_size(origin + vec2(0.0, top), vec2(width, *h));
                let name = names.iter().find(|(n, _)| n == who).map(|(_, s)| s.as_str()).unwrap_or("");
                let resp = ui.interact(rect, ui.id().with(("transcript-line", k)), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{} {name}: {}", clock(c.start), c.text)));
                let fill = if now == Some(k) {
                    t.accent.gamma_multiply(0.22)
                } else if resp.hovered() {
                    t.hover
                } else {
                    Color32::TRANSPARENT
                };
                let p = ui.painter();
                p.rect_filled(rect.shrink2(vec2(0.0, 1.0)), 6.0, fill);
                let when = p.text(rect.min + vec2(PAD, PAD), egui::Align2::LEFT_TOP, clock(c.start), egui::FontId::monospace(11.0), t.muted);
                p.text(
                    egui::pos2(when.max.x + 10.0, rect.min.y + PAD - 1.0),
                    egui::Align2::LEFT_TOP,
                    name,
                    crate::widgets::strong_font(&ctx, 12.5),
                    Self::who_colour(*who),
                );
                let words = p.layout(c.text.clone(), words_font.clone(), t.text, words_w);
                p.galley(rect.min + vec2(PAD, PAD + HEAD), words, t.text);
                if resp.on_hover_text("Go there").clicked() {
                    go = Some(c.start);
                }
            }
        });
        self.transcript_heights = Some((self.lines_rev, width, heights));
        if let Some(at) = go {
            let now = ctx.input(|i| i.time);
            self.seek(ed, at, now);
        }
    }
}
