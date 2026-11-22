//! QR codes (the Image menu and the right-click menu). **Read QR Codes** shows the words in the QR
//! codes the picture shows (or the area selected), each to copy; **Make a QR Code…** asks for words
//! or a web address and puts a code of them on the picture, selected, to move and size.
//!
//! Both are the engine's (`image.readQr`, `image.makeQr`), on this computer. A web address read is
//! shown as words and never opened from here: where it leads is the owner's to judge.

use egui::{RichText, Ui};
use serde_json::{Value, json};

use crate::editor::Editor;
use crate::theme::Tokens;

/// What the editor keeps for this: the codes read, or the words of one being made, in their window.
#[derive(Default)]
pub struct State {
    found: Option<Found>,
    making: Option<String>,
}

/// The codes read: what each says and whether it's a web address.
struct Found {
    codes: Vec<(String, bool)>,
    of_area: bool,
}

impl State {
    /// Whether a window of this has the editor's attention (its keys are left alone meanwhile).
    pub fn busy(&self) -> bool {
        self.found.is_some() || self.making.is_some()
    }

    /// What the codes read say, while their window is open.
    pub fn read(&self) -> Option<Vec<&str>> {
        self.found.as_ref().map(|f| f.codes.iter().map(|(t, _)| t.as_str()).collect())
    }
}

/// Read QR Codes (in the area when one is selected).
pub(crate) fn read_item(ed: &mut Editor, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let label = if ed.area.is_some() { "Read QR Codes in the Area" } else { "Read QR Codes" };
    if ui.add_enabled(!ed.qr.busy(), egui::Button::new(label)).on_hover_text("Reads the words in the QR codes in the picture, to copy").clicked() {
        read(ed, &ctx);
        ui.close();
    }
}

/// Make a QR Code…
pub(crate) fn make_item(ed: &mut Editor, ui: &mut Ui) {
    if ui
        .add_enabled(!ed.qr.busy(), egui::Button::new("Make a QR Code…"))
        .on_hover_text("Puts a QR code of your words or web address on the picture")
        .clicked()
    {
        ed.qr.making = Some(String::new());
        ui.close();
    }
}

/// Reads the codes in the picture as shown, or in the area selected; says so when there are none.
pub(crate) fn read(ed: &mut Editor, ctx: &egui::Context) {
    if ed.qr.busy() {
        return;
    }
    let area = ed.area_params();
    let of_area = area.is_some();
    let Some(out) = ed.run(ctx, "image.readQr", area.unwrap_or_else(|| json!({}))) else { return };
    let codes: Vec<(String, bool)> = out
        .get("codes")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|c| Some((c.get("text")?.as_str()?.to_string(), c.get("link").and_then(Value::as_bool).unwrap_or(false))))
                .collect()
        })
        .unwrap_or_default();
    if codes.is_empty() {
        ed.toast(ctx, if of_area { "No QR code was found in the area" } else { "No QR code was found in the picture" });
    } else {
        ed.qr.found = Some(Found { codes, of_area });
    }
}

/// Every frame: the windows, while they're open.
pub(crate) fn show(ed: &mut Editor, ctx: &egui::Context) {
    found_window(ed, ctx);
    make_window(ed, ctx);
}

fn found_window(ed: &mut Editor, ctx: &egui::Context) {
    let Some(found) = ed.qr.found.take() else { return };
    let t = Tokens::get(ctx);
    let mut open = true;
    let mut close = false;
    let mut copied: Option<String> = None;
    let size = ctx.content_rect().size();
    egui::Window::new("QR Codes")
        .collapsible(false)
        .resizable(true)
        .default_width((size.x * 0.45).clamp(320.0, 560.0))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            let what = if found.of_area { "the area" } else { "the picture" };
            let n = found.codes.len();
            ui.label(if n == 1 { format!("1 QR code in {what} says:") } else { format!("{n} QR codes in {what} say:") });
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height((size.y * 0.5).max(160.0)).show(ui, |ui| {
                for (i, (text, link)) in found.codes.iter().enumerate() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let mut shown = text.as_str();
                        ui.add(egui::TextEdit::multiline(&mut shown).desired_width(f32::INFINITY).desired_rows(1).id_salt(("qr-code", i)));
                        ui.horizontal(|ui| {
                            let copy = ui.button("Copy").on_hover_text("Puts these words on the clipboard");
                            copy.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Copy code {}", i + 1)));
                            if copy.clicked() {
                                copied = Some(text.clone());
                            }
                            if *link {
                                ui.label(
                                    RichText::new("A web address: it isn't opened from here. Copy it to your browser if you trust it.")
                                        .size(12.0)
                                        .color(t.muted),
                                );
                            }
                        });
                    });
                    ui.add_space(4.0);
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if n > 1 && ui.button("Copy All").clicked() {
                    copied = Some(found.codes.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>().join("\n"));
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        });
    if let Some(text) = copied {
        ctx.copy_text(text);
        ed.toast(ctx, "The words are on the clipboard");
    }
    if open && !close && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ed.qr.found = Some(found);
    }
}

fn make_window(ed: &mut Editor, ctx: &egui::Context) {
    let Some(mut text) = ed.qr.making.take() else { return };
    let t = Tokens::get(ctx);
    let mut open = true;
    let (mut make, mut cancel) = (false, false);
    egui::Window::new("Make a QR Code")
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("The words or web address to put in the code");
            ui.add_space(4.0);
            let edit = ui.add(
                egui::TextEdit::multiline(&mut text)
                    .desired_width(f32::INFINITY)
                    .desired_rows(3)
                    .char_limit(screengrabcraft_render::qr::MAX_TEXT)
                    .hint_text("https://example.com")
                    .id_salt("qr-words"),
            );
            edit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Words or a web address"));
            if !edit.has_focus() && text.is_empty() {
                edit.request_focus();
            }
            ui.label(
                RichText::new(format!(
                    "{} of {} characters. The code goes in the middle of the picture, to move and size like any picture.",
                    text.chars().count(),
                    screengrabcraft_render::qr::MAX_TEXT
                ))
                .size(12.0)
                .color(t.muted),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!text.trim().is_empty(), egui::Button::new("Make").min_size(egui::vec2(72.0, 0.0))).clicked() {
                    make = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    if make {
        if ed.run(ctx, "image.makeQr", json!({ "text": text })).is_some() {
            ed.toast(ctx, "The QR code is on the picture");
        }
        return;
    }
    if open && !cancel && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ed.qr.making = Some(text);
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT as _, Queryable as _};
    use screengrabcraft_model::{Color, Document, EditorSettings, Kind, Raster, Rect};

    use crate::editor::Editor;
    use crate::services::NoServices;

    fn harness() -> Harness<'static, Editor> {
        let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
        ed.session.replace(Document::new(Raster::filled(600, 400, Color::WHITE).unwrap()));
        Harness::builder().with_size(egui::vec2(1100.0, 760.0)).build_ui_state(
            |ui, ed: &mut Editor| {
                let ctx = ui.ctx().clone();
                ed.logic(&ctx);
                ed.ui(ui);
            },
            ed,
        )
    }

    fn shot(h: &mut Harness<'_, Editor>, name: &str) {
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/{name}.png"));
        }
    }

    #[test]
    fn a_code_is_made_from_words_then_read_back_to_copy() {
        let mut h = harness();
        h.run_steps(3);
        h.get_by_label("Image").click();
        h.run_steps(2);
        h.get_by_label("Make a QR Code…").click();
        h.run_steps(3);
        h.get_by_label("Words or a web address").type_text("https://example.com/menu");
        h.run_steps(2);
        shot(&mut h, "qr-make");
        h.get_by_label("Make").click();
        h.run_steps(3);
        let doc = &h.state().session.doc;
        assert_eq!(doc.objects.len(), 1);
        assert_eq!(doc.objects[0].shape.kind(), Kind::Image);
        assert!(!h.state().qr.busy(), "the window closes");
        // Read back: its words, a web address said to be one and not opened.
        h.get_by_label("Image").click();
        h.run_steps(2);
        h.get_by_label("Read QR Codes").click();
        h.run_steps(3);
        assert_eq!(h.state().qr.read(), Some(vec!["https://example.com/menu"]));
        h.get_by_label_contains("it isn't opened from here");
        shot(&mut h, "qr-read");
        h.get_by_label("Copy code 1").click();
        h.run_steps(2);
        assert!(h.state().toast_text().is_some_and(|t| t.contains("clipboard")));
        h.get_by_label("Close").click();
        h.run_steps(2);
        assert!(!h.state().qr.busy());
    }

    #[test]
    fn an_area_without_a_code_says_so_and_nothing_made_from_nothing() {
        let mut h = harness();
        h.run_steps(3);
        let ctx = h.ctx.clone();
        super::read(h.state_mut(), &ctx);
        assert!(h.state().toast_text().is_some_and(|t| t.contains("No QR code was found in the picture")));
        h.state_mut().area = Some(Rect::new(10.0, 10.0, 100.0, 80.0));
        super::read(h.state_mut(), &ctx);
        assert!(h.state().toast_text().is_some_and(|t| t.contains("in the area")));
        // The Make button waits for words.
        h.state_mut().area = None;
        h.state_mut().qr.making = Some("   ".to_string());
        h.run_steps(2);
        assert!(h.get_by_label("Make").accesskit_node().is_disabled());
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        assert!(h.state().session.doc.objects.is_empty());
        assert!(!h.state().qr.busy());
    }
}
