//! The transcript pane at the right of the video editor: everything said in the video as edited,
//! line by line, each with when it is and who said it (You on the microphone, Others on the
//! computer's sound, a layer's name for what was put on it). A click on a line takes the playhead
//! there; the line playing is marked, and followed while the video plays. Each speaker's name is
//! the name of the track (or the layer) it was heard on, and is renamed here or in its header.

//!
//! The recording's own lines are put right here too: a double click on a line puts its words in a
//! box to change (and to split at the cursor); its right button splits it, joins it to the
//! speaker's next line, moves its start or end to the playhead, or deletes it. Every change is a
//! `video.caption*` command, so it's undone like any edit and saved with the edits.
//!
//! Words are cut out of the video from here too: Cut Words from the Video (a line's right
//! button) shows its words to choose, and Take Out Filler Words cuts every um, uh and er the
//! transcript has (`video.cutWords`, `video.cutFillers`). The speech model times lines only, so a
//! word's time is worked out from its line's, by its length.

use egui::{Color32, Key, Rect, RichText, Sense, Ui, vec2};
use screengrabcraft_media::captions::{self, Cue, Speaker, Who};
use serde_json::json;

use super::{Rename, VideoView, clock, kind_colour};
use crate::editor::Editor;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

/// A line's words being put right in the transcript pane.
pub(crate) struct CaptionEdit {
    /// Which of the recording's captions it is (its place among them).
    pub source: usize,
    pub words: String,
    /// The box has had the keys (its words chosen, to type over).
    pub started: bool,
    /// Where the cursor is in the words (characters): Split at the Cursor cuts there.
    pub cursor: Option<usize>,
}

/// Words of a line being chosen in the transcript pane, to cut out of the video.
pub(crate) struct WordCut {
    /// Which of the recording's captions it is (its place among them).
    pub source: usize,
    /// The places of the words chosen.
    pub chosen: std::collections::BTreeSet<usize>,
}

/// What was asked of a line of the recording's captions in the pane.
enum LineAct {
    Edit(usize),
    CutWords(usize),
    Split(usize),
    Join(usize),
    /// Its start (true) or its end moved to the playhead; who said it, for whose sound it's on.
    Move(usize, Who, bool),
    Delete(usize),
    AllAsHeard,
}

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
                if self.line_sources.iter().any(Option::is_some) {
                    ui.add_space(4.0);
                    if ui
                        .button("Take Out Filler Words")
                        .on_hover_text("Cuts um, uh, er and the like out of the video wherever the transcript has them")
                        .clicked()
                    {
                        let ctx = ui.ctx().clone();
                        self.take_out_fillers(ed, &ctx);
                    }
                }
                ui.add_space(6.0);
                ui.separator();
                self.caption_editor(ed, ui);
                self.word_cutter(ed, ui);
                self.transcript_lines(ed, ui, &lines);
            });
    }

    /// The words of the line being put right, in a box above the lines: Enter (or Done) keeps
    /// them, Esc (or Cancel) lets them go, Split at the Cursor cuts the line in two there.
    fn caption_editor(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let Some(mut e) = self.caption_edit.take() else { return };
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        // Esc lets it go (seen before the box, which takes the key, has its turn).
        if e.started && ctx.input(|i| i.key_pressed(Key::Escape)) {
            return;
        }
        ui.label(RichText::new("Put the line right").size(12.0).color(t.muted));
        let out = egui::TextEdit::multiline(&mut e.words).desired_rows(2).desired_width(f32::INFINITY).return_key(None).show(ui);
        if !e.started {
            out.response.request_focus();
            let mut state = out.state.clone();
            let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(e.words.chars().count()));
            state.cursor.set_char_range(Some(all));
            state.store(&ctx, out.response.id);
            e.started = true;
        }
        if let Some(r) = out.cursor_range {
            e.cursor = Some(r.primary.index.into());
        }
        let enter = out.response.has_focus() && ctx.input(|i| i.key_pressed(Key::Enter));
        let (mut split, mut done, mut cancel) = (false, false, false);
        ui.horizontal(|ui| {
            split = ui.button("Split at the Cursor").on_hover_text("Two lines: the words before the cursor, and after it").clicked();
            done = ui.button("Done").on_hover_text("Keep the words (Enter)").clicked();
            cancel = ui.button("Cancel").on_hover_text("As they were (Esc)").clicked();
        });
        ui.add_space(6.0);
        ui.separator();
        if cancel {
            return;
        }
        if enter || done || split {
            self.run(ed, &ctx, "video.captionText", json!({"line": e.source, "text": e.words}));
            if split {
                // Where the cursor is in the words as they're kept (spaces tidied the same way).
                let before: String = e.words.chars().take(e.cursor.unwrap_or(0)).collect();
                let at = captions::clean_text(&before).chars().count();
                self.run(ed, &ctx, "video.captionSplit", json!({"line": e.source, "at": at}));
            }
            return;
        }
        self.caption_edit = Some(e);
    }

    /// The words of a line, to choose those to cut out of the video, above the lines: a click
    /// chooses a word (or lets it go), Cut from the Video (or Delete) takes them out, Esc (or
    /// Cancel) leaves them.
    fn word_cutter(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let Some(mut w) = self.word_cut.take() else { return };
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let said = captions::as_corrected(self.captions.as_ref(), &self.session.timeline);
        let Some(cue) = said.as_ref().and_then(|s| s.cues.get(w.source)).cloned() else { return };
        let words = captions::words(&cue);
        ui.label(RichText::new("Cut words out of the video: click the words to take out").size(12.0).color(t.muted));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 4.0);
            for (i, word) in words.iter().enumerate() {
                let on = w.chosen.contains(&i);
                if ui.selectable_label(on, &word.text).on_hover_text(format!("Said at about {}", clock(word.start))).clicked() {
                    if on {
                        w.chosen.remove(&i);
                    } else {
                        w.chosen.insert(i);
                    }
                }
            }
        });
        let (mut cut, mut fillers, mut cancel) = (false, false, false);
        ui.horizontal(|ui| {
            cut = ui
                .add_enabled(!w.chosen.is_empty(), egui::Button::new("Cut from the Video"))
                .on_hover_text("Where they're said is taken out of every track, and they go from the line (Delete)")
                .clicked();
            fillers = ui.button("Filler Words").on_hover_text("Choose its um, uh, er and the like").clicked();
            cancel = ui.button("Cancel").on_hover_text("Nothing is cut (Esc)").clicked();
        });
        ui.label(RichText::new("A word's time is worked out from its line's, by its length.").size(11.0).color(t.muted));
        ui.add_space(6.0);
        ui.separator();
        if cancel {
            return;
        }
        if fillers {
            let list: Vec<String> = captions::FILLERS.iter().map(|s| s.to_string()).collect();
            w.chosen.extend(words.iter().enumerate().filter(|(_, x)| list.contains(&captions::bare(&x.text))).map(|(i, _)| i));
        }
        self.word_cut = Some(w);
        if cut {
            self.cut_chosen_words(ed, &ctx);
        }
    }

    /// The words chosen in the transcript cut out of the video; the playhead goes where they were.
    pub(super) fn cut_chosen_words(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let Some(w) = self.word_cut.take() else { return };
        self.pause();
        let words: Vec<usize> = w.chosen.into_iter().collect();
        let r = self.session.execute("video.cutWords", &json!({"line": w.source, "words": words}));
        self.after_word_cut(ed, ctx, r, "word");
    }

    /// Every filler word (um, uh, er…) the transcript has cut out of the video.
    pub(super) fn take_out_fillers(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        self.word_cut = None;
        self.caption_edit = None;
        self.pause();
        let r = self.session.execute("video.cutFillers", &json!({}));
        self.after_word_cut(ed, ctx, r, "filler word");
    }

    /// Said after words were cut: how many, and how much shorter the video is; the playhead where
    /// the first was.
    fn after_word_cut(&mut self, ed: &mut Editor, ctx: &egui::Context, r: screengrabcraft_engine::Result<serde_json::Value>, what: &str) {
        match r {
            Ok(v) => {
                let n = v.get("words").and_then(serde_json::Value::as_u64).unwrap_or(0);
                let cut = v.get("cut").and_then(serde_json::Value::as_u64).unwrap_or(0);
                if let Some(at) = v.get("at").and_then(serde_json::Value::as_u64) {
                    self.playhead = at.min(self.length());
                }
                self.range = None;
                let s = if n == 1 { "" } else { "s" };
                ed.toast(ctx, format!("{n} {what}{s} cut out of the video ({:.1} s)", cut as f64 / 1e6));
            }
            Err(e) => ed.error(ctx, e.to_string()),
        }
        self.fix_selection();
    }

    /// A line of the recording's captions, to put right: its words in the box above the lines.
    fn edit_caption(&mut self, source: usize) {
        let said = captions::as_corrected(self.captions.as_ref(), &self.session.timeline);
        if let Some(c) = said.as_ref().and_then(|s| s.cues.get(source)) {
            self.caption_edit = Some(CaptionEdit { source, words: c.text.clone(), started: false, cursor: None });
        }
    }

    /// Does what was asked of a line in the pane.
    fn line_act(&mut self, ed: &mut Editor, ctx: &egui::Context, act: LineAct) {
        // Whatever was being typed is let go (a new line's words take its place).
        self.caption_edit = None;
        self.word_cut = None;
        match act {
            LineAct::Edit(k) => self.edit_caption(k),
            LineAct::CutWords(k) => self.word_cut = Some(WordCut { source: k, chosen: Default::default() }),
            LineAct::Split(k) => self.run(ed, ctx, "video.captionSplit", json!({"line": k})),
            LineAct::Join(k) => self.run(ed, ctx, "video.captionJoin", json!({"line": k})),
            LineAct::Delete(k) => self.run(ed, ctx, "video.captionText", json!({"line": k, "text": ""})),
            LineAct::AllAsHeard => self.run(ed, ctx, "video.captionsHeard", json!({})),
            LineAct::Move(k, who, start) => {
                // The playhead's moment in the recording, on the sound the line was heard on.
                let tl = &self.session.timeline;
                let at = match who {
                    Who::Speaker(s) => tl.find(s.source()).and_then(|i| tl.source_time(i, self.playhead)),
                    Who::Layer(_) => None,
                };
                let key = if start { "start" } else { "end" };
                match at {
                    Some(t) => self.run(ed, ctx, "video.captionTimes", json!({"line": k, key: t})),
                    None => ed.error(ctx, "The playhead isn't on that speaker's sound: move it onto a clip of their track first".to_string()),
                }
            }
        }
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
        let mut act = None;
        let sources = self.line_sources.clone();
        let editing = self.caption_edit.as_ref().map(|e| e.source).or(self.word_cut.as_ref().map(|w| w.source));
        let corrected = self.session.timeline.corrected.is_some();
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
                let source = sources.get(k).copied().flatten();
                let fill = if source.is_some() && source == editing {
                    t.accent.gamma_multiply(0.4)
                } else if now == Some(k) {
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
                let Some(source) = source else {
                    if resp.on_hover_text("Go there").clicked() {
                        go = Some(c.start);
                    }
                    continue;
                };
                // The recording's own lines: put right with a double click or the right button.
                let resp = resp.on_hover_text("Go there; double-click to put its words right, right-click for more");
                if resp.double_clicked() {
                    act = Some(LineAct::Edit(source));
                } else if resp.clicked() {
                    go = Some(c.start);
                }
                resp.context_menu(|ui| {
                    let mut pick = |ui: &mut Ui, label: &str, tip: &str, what: LineAct| {
                        if ui.button(label).on_hover_text(tip).clicked() {
                            act = Some(what);
                            ui.close();
                        }
                    };
                    pick(ui, "Put the Words Right…", "Change its words (or double-click the line)", LineAct::Edit(source));
                    pick(
                        ui,
                        "Cut Words from the Video…",
                        "Choose words of the line: where they're said is taken out of the video",
                        LineAct::CutWords(source),
                    );
                    pick(ui, "Split in Two", "Two lines, between the words nearest its middle", LineAct::Split(source));
                    pick(ui, "Join with the Next Line", "With the same speaker's next line", LineAct::Join(source));
                    ui.separator();
                    pick(ui, "Start at the Playhead", "The line starts where the playhead is", LineAct::Move(source, *who, true));
                    pick(ui, "End at the Playhead", "The line ends where the playhead is", LineAct::Move(source, *who, false));
                    ui.separator();
                    pick(ui, "Delete the Line", "Its words go from the captions (the sound stays)", LineAct::Delete(source));
                    if corrected {
                        pick(ui, "All Captions as Heard", "Every line put right goes back to how it was heard", LineAct::AllAsHeard);
                    }
                });
            }
        });
        self.transcript_heights = Some((self.lines_rev, width, heights));
        if let Some(at) = go {
            let now = ctx.input(|i| i.time);
            self.seek(ed, at, now);
        }
        if let Some(a) = act {
            self.line_act(ed, &ctx, a);
        }
    }
}
