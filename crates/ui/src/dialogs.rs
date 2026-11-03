//! The small windows: picture size, canvas size, the effects' settings, shortcuts, about.

use egui::{Color32, Ui};
use screengrabcraft_model::Document;
use serde_json::{Value, json};

use crate::editor::Editor;
use crate::from_color32;
use crate::theme::Tokens;
use crate::widgets::{self, FooterButton, Role};

#[derive(Clone, Debug)]
pub enum Dialog {
    Resize { width: u32, height: u32, keep: bool, aspect: f32 },
    Canvas { left: i32, top: i32, right: i32, bottom: i32, color: Color32 },
    Border { width: u32, color: Color32 },
    Shadow { offset: u32, softness: u32, color: Color32 },
    Torn { top: bool, right: bool, bottom: bool, left: bool, depth: u32 },
    Round { radius: u32 },
    Brightness { brightness: f32, contrast: f32 },
    Shortcuts,
    About,
}

impl Dialog {
    pub fn resize(doc: &Document) -> Dialog {
        let (w, h) = (doc.width(), doc.height());
        Dialog::Resize { width: w, height: h, keep: true, aspect: w as f32 / h.max(1) as f32 }
    }

    pub fn canvas() -> Dialog {
        Dialog::Canvas { left: 20, top: 20, right: 20, bottom: 20, color: Color32::WHITE }
    }

    pub fn border() -> Dialog {
        Dialog::Border { width: 4, color: Color32::from_rgb(0x30, 0x30, 0x36) }
    }

    pub fn shadow() -> Dialog {
        Dialog::Shadow { offset: 8, softness: 10, color: Color32::from_rgba_unmultiplied(0, 0, 0, 120) }
    }

    pub fn torn() -> Dialog {
        Dialog::Torn { top: false, right: false, bottom: true, left: false, depth: 14 }
    }

    pub fn round() -> Dialog {
        Dialog::Round { radius: 16 }
    }

    pub fn brightness() -> Dialog {
        Dialog::Brightness { brightness: 0.0, contrast: 0.0 }
    }

    fn title(&self) -> &'static str {
        match self {
            Dialog::Resize { .. } => "Resize Image",
            Dialog::Canvas { .. } => "Canvas Size",
            Dialog::Border { .. } => "Border",
            Dialog::Shadow { .. } => "Drop Shadow",
            Dialog::Torn { .. } => "Torn Edge",
            Dialog::Round { .. } => "Round Corners",
            Dialog::Brightness { .. } => "Brightness and Contrast",
            Dialog::Shortcuts => "Keyboard Shortcuts",
            Dialog::About => "About",
        }
    }

    /// How wide its window is (fixed, so its footer can put the buttons at the right edge).
    fn width(&self) -> f32 {
        match self {
            Dialog::Shortcuts => 560.0,
            Dialog::About => 420.0,
            _ => 380.0,
        }
    }
}

/// Opens `dialog` in the editor (what its menu items do).
pub fn open(ed: &mut Editor, dialog: Dialog) {
    ed.dialog = Some(dialog);
}

fn hex(c: Color32) -> String {
    from_color32(c).to_hex()
}

/// The two columns of a dialog's settings: the names on the left, the controls on the right.
fn settings<R>(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Grid::new(id).num_columns(2).spacing([16.0, 10.0]).min_col_width(72.0).show(ui, add).inner
}

/// The dialog's controls; the command to run when it is applied.
fn body(d: &mut Dialog, ui: &mut Ui) -> Option<(&'static str, Value)> {
    match d {
        Dialog::Resize { width, height, keep, aspect } => {
            settings(ui, "resize", |ui| {
                ui.label("Width");
                if ui.add(egui::DragValue::new(width).range(1..=65_535).suffix(" px")).changed() && *keep {
                    *height = ((*width as f32 / *aspect).round() as u32).max(1);
                }
                ui.end_row();
                ui.label("Height");
                if ui.add(egui::DragValue::new(height).range(1..=65_535).suffix(" px")).changed() && *keep {
                    *width = ((*height as f32 * *aspect).round() as u32).max(1);
                }
                ui.end_row();
                ui.label("");
                ui.checkbox(keep, "Keep proportions");
                ui.end_row();
                ui.label("Scale");
                ui.horizontal(|ui| {
                    for pct in [25u32, 50, 75, 150, 200] {
                        if ui.button(format!("{pct}%")).clicked() {
                            *width = ((*width as f32) * pct as f32 / 100.0).round().max(1.0) as u32;
                            *height = ((*width as f32) / *aspect).round().max(1.0) as u32;
                        }
                    }
                });
                ui.end_row();
            });
            Some(("image.resize", json!({"width": width, "height": height})))
        }
        Dialog::Canvas { left, top, right, bottom, color } => {
            settings(ui, "canvas", |ui| {
                for (label, v) in [("Left", &mut *left), ("Top", &mut *top), ("Right", &mut *right), ("Bottom", &mut *bottom)] {
                    ui.label(label);
                    ui.add(egui::DragValue::new(v).range(-10_000..=10_000).suffix(" px"));
                    ui.end_row();
                }
                ui.label("New area");
                egui::color_picker::color_edit_button_srgba(ui, color, egui::color_picker::Alpha::BlendOrAdditive);
                ui.end_row();
            });
            ui.add_space(8.0);
            ui.label(egui::RichText::new("A negative number takes that much away.").small().color(Tokens::get(ui.ctx()).muted));
            Some(("image.canvas", json!({"left": left, "top": top, "right": right, "bottom": bottom, "color": hex(*color)})))
        }
        Dialog::Border { width, color } => {
            settings(ui, "border", |ui| {
                ui.label("Width");
                ui.add(egui::Slider::new(width, 1..=100).suffix(" px"));
                ui.end_row();
                ui.label("Colour");
                egui::color_picker::color_edit_button_srgba(ui, color, egui::color_picker::Alpha::Opaque);
                ui.end_row();
            });
            Some(("effect.border", json!({"width": width, "color": hex(*color)})))
        }
        Dialog::Shadow { offset, softness, color } => {
            settings(ui, "shadow", |ui| {
                ui.label("Offset");
                ui.add(egui::Slider::new(offset, 0..=60).suffix(" px"));
                ui.end_row();
                ui.label("Softness");
                ui.add(egui::Slider::new(softness, 1..=60));
                ui.end_row();
                ui.label("Colour");
                egui::color_picker::color_edit_button_srgba(ui, color, egui::color_picker::Alpha::BlendOrAdditive);
                ui.end_row();
            });
            Some(("effect.shadow", json!({"offset": offset, "softness": softness, "color": hex(*color)})))
        }
        Dialog::Torn { top, right, bottom, left, depth } => {
            settings(ui, "torn", |ui| {
                ui.label("Tear");
                ui.horizontal(|ui| {
                    ui.checkbox(top, "Top");
                    ui.checkbox(right, "Right");
                    ui.checkbox(bottom, "Bottom");
                    ui.checkbox(left, "Left");
                });
                ui.end_row();
                ui.label("Depth");
                ui.add(egui::Slider::new(depth, 4..=80).suffix(" px"));
                ui.end_row();
            });
            Some(("effect.tornEdge", json!({"top": top, "right": right, "bottom": bottom, "left": left, "depth": depth})))
        }
        Dialog::Round { radius } => {
            settings(ui, "round", |ui| {
                ui.label("Radius");
                ui.add(egui::Slider::new(radius, 1..=200).suffix(" px"));
                ui.end_row();
            });
            Some(("effect.roundCorners", json!({"radius": radius})))
        }
        Dialog::Brightness { brightness, contrast } => {
            settings(ui, "brightness", |ui| {
                ui.label("Brightness");
                ui.add(egui::Slider::new(brightness, -100.0..=100.0));
                ui.end_row();
                ui.label("Contrast");
                ui.add(egui::Slider::new(contrast, -100.0..=100.0));
                ui.end_row();
            });
            Some(("effect.brightness", json!({"brightness": brightness, "contrast": contrast})))
        }
        Dialog::Shortcuts => {
            // Taller than a small window: the list scrolls, the footer stays in sight.
            let room = (ui.ctx().viewport_rect().height() - 200.0).max(160.0);
            egui::ScrollArea::vertical().max_height(room).show(ui, shortcuts);
            None
        }
        Dialog::About => {
            let t = Tokens::get(ui.ctx());
            ui.label(egui::RichText::new(screengrabcraft_model::EDITOR_NAME).font(widgets::strong_font(ui.ctx(), 18.0)));
            ui.label(egui::RichText::new(format!("Version {}", screengrabcraft_model::VERSION)).color(t.muted));
            ui.add_space(10.0);
            ui.label("Annotate, mark up and share screenshots. Part of ScreenGrabCraft, an open-source screen capture suite written in Rust.");
            ui.add_space(8.0);
            ui.hyperlink_to("github.com/billsecond/screengrabcraft", screengrabcraft_model::HOMEPAGE);
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("MIT OR Apache-2.0. Fonts: PatroneOS Sans and PatroneOS Mono (SIL Open Font Licence).").small().color(t.muted),
            );
            None
        }
    }
}

/// Every key the editor knows, and what it does.
fn shortcuts(ui: &mut Ui) {
    egui::Grid::new("keys").num_columns(2).striped(true).spacing([18.0, 4.0]).show(ui, |ui| {
        let mut row = |k: &str, what: &str| {
            ui.label(egui::RichText::new(k).monospace());
            ui.label(what);
            ui.end_row();
        };
        for (_, _, label, key) in crate::editor::Tool::BAR {
            row(key.name(), label);
        }
        for (k, what) in [
            ("Shift (while drawing)", "Straight lines at 45°, squares and circles"),
            ("Double-click", "Type into a text box or callout"),
            ("Arrows / Shift+Arrows", "Nudge the selection 1 / 10 px"),
            ("Delete", "Delete the selection"),
            ("Ctrl+Z / Ctrl+Y", "Undo / Redo"),
            ("Ctrl+C", "Copy the picture with its annotations"),
            ("Ctrl+V", "Paste a picture into this one"),
            ("Ctrl+Shift+V", "Paste a picture as a new one"),
            ("Ctrl+D", "Duplicate"),
            ("Ctrl+S / Ctrl+Shift+S", "Save / Save As"),
            ("Ctrl+E", "Export"),
            ("Ctrl+O", "Open"),
            ("Ctrl+Shift+N", "New capture"),
            ("Ctrl+0 / Ctrl+1", "Zoom to fit / actual size"),
            ("Ctrl+wheel, Ctrl+= / Ctrl+-", "Zoom"),
            ("Space+drag, middle drag", "Pan"),
            ("Enter / Esc", "Apply / cancel a crop"),
        ] {
            row(k, what);
        }
    });
}

/// Shows the open dialog, if any, as a standard dialog ([`widgets`]): its title and an X, its
/// settings, and [Apply] [Cancel] at the bottom right, or a single Close when there is nothing to
/// apply. Apply (or Enter) runs its command; Cancel, the X or Esc close it.
pub fn show(ed: &mut Editor, ctx: &egui::Context) {
    let Some(mut d) = ed.dialog.take() else { return };
    let mut apply: Option<(&'static str, Value)> = None;
    let mut close = false;
    let width = d.width();
    egui::Window::new(d.title())
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(width)
        .max_width(width)
        .frame(widgets::window_frame(&ctx.global_style()))
        .show(ctx, |ui| {
            if widgets::title_bar(ui, d.title()) {
                close = true;
            }
            let cmd = widgets::content(ui, |ui| body(&mut d, ui));
            let buttons: &[FooterButton<'_>] = match cmd {
                Some(_) => &[FooterButton::primary("Apply"), FooterButton::cancel("Cancel")],
                None => &[FooterButton::primary("Close")],
            };
            let pressed = widgets::dialog_footer(ui, buttons).and_then(|i| buttons.get(i)).map(|b| b.role);
            match (pressed, cmd) {
                (Some(Role::Default), Some(c)) => apply = Some(c),
                (Some(_), _) => close = true,
                (None, _) => {}
            }
        });
    if let Some((id, params)) = apply {
        if ed.run(ctx, id, params).is_some() {
            ed.view.fit_pending = true;
        }
        return;
    }
    if !close {
        ed.dialog = Some(d);
    }
}
