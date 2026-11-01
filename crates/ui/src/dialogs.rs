//! The small windows: picture size, canvas size, the effects' settings, shortcuts, about.

use egui::{Color32, Ui};
use screengrabcraft_model::Document;
use serde_json::{Value, json};

use crate::editor::Editor;
use crate::from_color32;

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
}

fn hex(c: Color32) -> String {
    from_color32(c).to_hex()
}

/// The dialog's controls; the command to run when it is applied.
fn body(d: &mut Dialog, ui: &mut Ui) -> Option<(&'static str, Value)> {
    match d {
        Dialog::Resize { width, height, keep, aspect } => {
            egui::Grid::new("resize").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
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
            });
            ui.checkbox(keep, "Keep proportions");
            ui.horizontal(|ui| {
                for pct in [25u32, 50, 75, 150, 200] {
                    if ui.button(format!("{pct}%")).clicked() {
                        *width = ((*width as f32) * pct as f32 / 100.0).round().max(1.0) as u32;
                        *height = ((*width as f32) / *aspect).round().max(1.0) as u32;
                    }
                }
            });
            Some(("image.resize", json!({"width": width, "height": height})))
        }
        Dialog::Canvas { left, top, right, bottom, color } => {
            egui::Grid::new("canvas").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                for (label, v) in [("Left", &mut *left), ("Top", &mut *top), ("Right", &mut *right), ("Bottom", &mut *bottom)] {
                    ui.label(label);
                    ui.add(egui::DragValue::new(v).range(-10_000..=10_000).suffix(" px"));
                    ui.end_row();
                }
                ui.label("New area");
                egui::color_picker::color_edit_button_srgba(ui, color, egui::color_picker::Alpha::BlendOrAdditive);
                ui.end_row();
            });
            ui.label(egui::RichText::new("A negative number takes that much away.").small());
            Some(("image.canvas", json!({"left": left, "top": top, "right": right, "bottom": bottom, "color": hex(*color)})))
        }
        Dialog::Border { width, color } => {
            ui.add(egui::Slider::new(width, 1..=100).text("Width (px)"));
            ui.horizontal(|ui| {
                ui.label("Colour");
                egui::color_picker::color_edit_button_srgba(ui, color, egui::color_picker::Alpha::Opaque);
            });
            Some(("effect.border", json!({"width": width, "color": hex(*color)})))
        }
        Dialog::Shadow { offset, softness, color } => {
            ui.add(egui::Slider::new(offset, 0..=60).text("Offset (px)"));
            ui.add(egui::Slider::new(softness, 1..=60).text("Softness"));
            ui.horizontal(|ui| {
                ui.label("Colour");
                egui::color_picker::color_edit_button_srgba(ui, color, egui::color_picker::Alpha::BlendOrAdditive);
            });
            Some(("effect.shadow", json!({"offset": offset, "softness": softness, "color": hex(*color)})))
        }
        Dialog::Torn { top, right, bottom, left, depth } => {
            ui.label("Tear these edges:");
            ui.horizontal(|ui| {
                ui.checkbox(top, "Top");
                ui.checkbox(right, "Right");
                ui.checkbox(bottom, "Bottom");
                ui.checkbox(left, "Left");
            });
            ui.add(egui::Slider::new(depth, 4..=80).text("Depth (px)"));
            Some(("effect.tornEdge", json!({"top": top, "right": right, "bottom": bottom, "left": left, "depth": depth})))
        }
        Dialog::Round { radius } => {
            ui.add(egui::Slider::new(radius, 1..=200).text("Radius (px)"));
            Some(("effect.roundCorners", json!({"radius": radius})))
        }
        Dialog::Brightness { brightness, contrast } => {
            ui.add(egui::Slider::new(brightness, -100.0..=100.0).text("Brightness"));
            ui.add(egui::Slider::new(contrast, -100.0..=100.0).text("Contrast"));
            Some(("effect.brightness", json!({"brightness": brightness, "contrast": contrast})))
        }
        Dialog::Shortcuts => {
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
            None
        }
        Dialog::About => {
            ui.heading(screengrabcraft_model::EDITOR_NAME);
            ui.label(format!("Version {}", screengrabcraft_model::VERSION));
            ui.add_space(6.0);
            ui.label("Annotate, mark up and share screenshots. Part of ScreenGrabCraft, an open-source screen capture suite written in Rust.");
            ui.add_space(6.0);
            ui.hyperlink_to("github.com/billsecond/screengrabcraft", screengrabcraft_model::HOMEPAGE);
            ui.add_space(6.0);
            ui.label(egui::RichText::new("MIT OR Apache-2.0. Fonts: PatroneOS Sans and PatroneOS Mono (SIL Open Font Licence).").small());
            None
        }
    }
}

/// Shows the open dialog, if any; Apply runs its command, Cancel closes it.
pub fn show(ed: &mut Editor, ctx: &egui::Context) {
    let Some(mut d) = ed.dialog.take() else { return };
    let mut open = true;
    let mut apply: Option<(&'static str, Value)> = None;
    let mut close = false;
    egui::Window::new(d.title()).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).open(&mut open).show(
        ctx,
        |ui| {
            ui.set_min_width(300.0);
            let cmd = body(&mut d, ui);
            ui.add_space(10.0);
            ui.horizontal(|ui| match cmd {
                Some(c) => {
                    if ui.button("Apply").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        apply = Some(c);
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                }
                None => {
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
            });
        },
    );
    if let Some((id, params)) = apply {
        if ed.run(ctx, id, params).is_some() {
            ed.view.fit_pending = true;
        }
        return;
    }
    if open && !close && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ed.dialog = Some(d);
    }
}
