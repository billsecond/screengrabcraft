//! The small windows: picture size, canvas size, the effects' settings, shortcuts, about.

use egui::{Color32, Ui};
use screengrabcraft_model::Document;
use screengrabcraft_render::frame::Corner;
use screengrabcraft_render::marks::{DetailsPlace, Place};
use serde_json::{Value, json};

use crate::editor::Editor;
use crate::from_color32;
use crate::theme::Tokens;
use crate::widgets::{self, FooterButton, Role};

#[derive(Clone, Debug)]
pub enum Dialog {
    Resize {
        width: u32,
        height: u32,
        keep: bool,
        aspect: f32,
    },
    Canvas {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        color: Color32,
    },
    Border {
        width: u32,
        color: Color32,
    },
    Shadow {
        offset: u32,
        softness: u32,
        color: Color32,
    },
    Torn {
        top: bool,
        right: bool,
        bottom: bool,
        left: bool,
        depth: u32,
    },
    Round {
        radius: u32,
    },
    /// The picture turned by any angle; the corners round it see-through or a colour.
    RotateAny {
        degrees: f32,
        see_through: bool,
        color: Color32,
    },
    Brightness {
        brightness: f32,
        contrast: f32,
    },
    Wavy {
        top: bool,
        right: bool,
        bottom: bool,
        left: bool,
        depth: u32,
        length: u32,
    },
    Faded {
        top: bool,
        right: bool,
        bottom: bool,
        left: bool,
        width: u32,
    },
    Bevel {
        width: u32,
        strength: u32,
    },
    Perspective {
        horizontal: i32,
        vertical: i32,
    },
    Curl {
        corner: Corner,
        size: u32,
    },
    Reflection {
        height: u32,
        opacity: u32,
        gap: u32,
    },
    Hue {
        hue: f32,
        saturation: f32,
        lightness: f32,
        gamma: f32,
    },
    /// A colour replaced by another. `at` is the pixel the colour was picked on with the
    /// eyedropper (none when it was chosen otherwise): where "only where it joins" starts.
    Replace {
        from: Color32,
        with: Color32,
        tolerance: u8,
        joined: bool,
        at: Option<(u32, u32)>,
    },
    Sharpen {
        amount: u32,
        radius: f32,
    },
    Blur {
        radius: f32,
    },
    /// A watermark of words or of a picture (`picture` is the chosen one's name; the editor
    /// holds it). `choose` is set when Choose a Picture is pressed.
    Watermark {
        words: bool,
        text: String,
        picture: Option<String>,
        choose: bool,
        place: Place,
        size: u32,
        angle: i32,
        opacity: u32,
        under: bool,
        color: Color32,
    },
    /// The capture's details: each written or not, the time and what it is of as typed (as the
    /// editor knows them when left empty).
    Details {
        time: bool,
        time_text: String,
        program: bool,
        program_text: String,
        size: bool,
        place: DetailsPlace,
        font_size: u32,
        color: Color32,
        background: Color32,
    },
    Shortcuts,
    /// The video editor's keys.
    VideoShortcuts,
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

    pub fn rotate_any() -> Dialog {
        Dialog::RotateAny { degrees: 15.0, see_through: true, color: Color32::WHITE }
    }

    pub fn round() -> Dialog {
        Dialog::Round { radius: 16 }
    }

    pub fn brightness() -> Dialog {
        Dialog::Brightness { brightness: 0.0, contrast: 0.0 }
    }

    pub fn wavy() -> Dialog {
        Dialog::Wavy { top: false, right: false, bottom: true, left: false, depth: 10, length: 40 }
    }

    pub fn faded() -> Dialog {
        Dialog::Faded { top: true, right: true, bottom: true, left: true, width: 30 }
    }

    pub fn bevel() -> Dialog {
        Dialog::Bevel { width: 8, strength: 50 }
    }

    pub fn perspective() -> Dialog {
        Dialog::Perspective { horizontal: 30, vertical: 0 }
    }

    pub fn curl() -> Dialog {
        Dialog::Curl { corner: Corner::BottomRight, size: 20 }
    }

    pub fn reflection() -> Dialog {
        Dialog::Reflection { height: 35, opacity: 40, gap: 0 }
    }

    pub fn hue() -> Dialog {
        Dialog::Hue { hue: 0.0, saturation: 0.0, lightness: 0.0, gamma: 1.0 }
    }

    pub fn replace() -> Dialog {
        Dialog::Replace { from: Color32::WHITE, with: Color32::from_rgb(0xFF, 0xD6, 0x00), tolerance: 24, joined: false, at: None }
    }

    pub fn sharpen() -> Dialog {
        Dialog::Sharpen { amount: 60, radius: 1.5 }
    }

    pub fn blur() -> Dialog {
        Dialog::Blur { radius: 4.0 }
    }

    pub fn watermark() -> Dialog {
        Dialog::Watermark {
            words: true,
            text: "DRAFT".to_string(),
            picture: None,
            choose: false,
            place: Place::Center,
            size: 40,
            angle: -30,
            opacity: 30,
            under: false,
            color: Color32::WHITE,
        }
    }

    pub fn details() -> Dialog {
        Dialog::Details {
            time: true,
            time_text: String::new(),
            program: true,
            program_text: String::new(),
            size: true,
            place: DetailsPlace::BottomRight,
            font_size: 14,
            color: Color32::WHITE,
            background: Color32::from_rgba_unmultiplied(0, 0, 0, 160),
        }
    }

    /// Whether its command can be kept to the area selected on the picture.
    pub(crate) fn takes_area(id: &str) -> bool {
        matches!(
            id,
            "effect.grayscale"
                | "effect.invert"
                | "effect.sepia"
                | "effect.brightness"
                | "effect.hueSaturation"
                | "effect.replaceColor"
                | "effect.sharpen"
                | "effect.blur"
        )
    }

    pub(crate) fn title(&self) -> &'static str {
        match self {
            Dialog::Resize { .. } => "Resize Image",
            Dialog::Canvas { .. } => "Canvas Size",
            Dialog::Border { .. } => "Border",
            Dialog::Shadow { .. } => "Drop Shadow",
            Dialog::Torn { .. } => "Torn Edge",
            Dialog::Round { .. } => "Round Corners",
            Dialog::RotateAny { .. } => "Rotate by an Angle",
            Dialog::Brightness { .. } => "Brightness and Contrast",
            Dialog::Wavy { .. } => "Wavy Edge",
            Dialog::Faded { .. } => "Faded Edge",
            Dialog::Bevel { .. } => "Bevel",
            Dialog::Perspective { .. } => "Perspective",
            Dialog::Curl { .. } => "Page Curl",
            Dialog::Reflection { .. } => "Reflection",
            Dialog::Hue { .. } => "Hue, Saturation and Gamma",
            Dialog::Replace { .. } => "Replace a Colour",
            Dialog::Sharpen { .. } => "Sharpen",
            Dialog::Blur { .. } => "Blur",
            Dialog::Watermark { .. } => "Watermark",
            Dialog::Details { .. } => "Capture Details",
            Dialog::Shortcuts | Dialog::VideoShortcuts => "Keyboard Shortcuts",
            Dialog::About => "About",
        }
    }

    /// How wide its window is (fixed, so its footer can put the buttons at the right edge).
    fn width(&self) -> f32 {
        match self {
            Dialog::Shortcuts => 560.0,
            Dialog::VideoShortcuts => 640.0,
            Dialog::About => 420.0,
            _ => 380.0,
        }
    }
}

/// Opens `dialog` in the editor (what its menu items do).
pub fn open(ed: &mut Editor, dialog: Dialog) {
    ed.dialog = Some(dialog);
}

/// What Replace a Colour's eyedropper and its "only where it joins" box are called.
pub const REPLACE_PICK: &str = "Pick the colour to replace from the picture";
pub const REPLACE_JOINED: &str = "Only where it joins the point picked";

fn hex(c: Color32) -> String {
    from_color32(c).to_hex()
}

/// The two columns of a dialog's settings: the names on the left, the controls on the right.
fn settings<R>(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Grid::new(id).num_columns(2).spacing([16.0, 10.0]).min_col_width(72.0).show(ui, add).inner
}

/// Which sides an edge effect is on: two by two, so they fit a narrow pane too.
fn sides(ui: &mut Ui, top: &mut bool, right: &mut bool, bottom: &mut bool, left: &mut bool) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.checkbox(top, "Top");
            ui.checkbox(bottom, "Bottom");
        });
        ui.horizontal(|ui| {
            ui.checkbox(left, "Left");
            ui.checkbox(right, "Right");
        });
    });
}

/// The dialog's controls; the command to run when it is applied. The Effects pane shows the
/// effects' controls too.
pub(crate) fn body(d: &mut Dialog, ui: &mut Ui) -> Option<(&'static str, Value)> {
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
        Dialog::RotateAny { degrees, see_through, color } => {
            settings(ui, "rotate", |ui| {
                ui.label("Angle");
                ui.add(egui::Slider::new(degrees, -180.0..=180.0).step_by(0.5).suffix("°"));
                ui.end_row();
                ui.label("Corners");
                ui.horizontal(|ui| {
                    ui.checkbox(see_through, "See-through");
                    if !*see_through {
                        egui::color_picker::color_edit_button_srgba(ui, color, egui::color_picker::Alpha::Opaque);
                    }
                });
                ui.end_row();
            });
            ui.add_space(8.0);
            let how = "Clockwise; below nought, the other way. The picture grows to hold it, and the annotations turn with it.";
            ui.label(egui::RichText::new(how).small().color(Tokens::get(ui.ctx()).muted));
            let mut params = json!({"degrees": degrees});
            if !*see_through {
                params["color"] = json!(hex(*color));
            }
            Some(("image.rotate", params))
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
                sides(ui, top, right, bottom, left);
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
        Dialog::Wavy { top, right, bottom, left, depth, length } => {
            settings(ui, "wavy", |ui| {
                ui.label("Wave");
                sides(ui, top, right, bottom, left);
                ui.end_row();
                ui.label("Depth");
                ui.add(egui::Slider::new(depth, 2..=60).suffix(" px"));
                ui.end_row();
                ui.label("Wave length");
                ui.add(egui::Slider::new(length, 8..=200).suffix(" px"));
                ui.end_row();
            });
            Some(("effect.wavyEdge", json!({"top": top, "right": right, "bottom": bottom, "left": left, "depth": depth, "length": length})))
        }
        Dialog::Faded { top, right, bottom, left, width } => {
            settings(ui, "faded", |ui| {
                ui.label("Fade");
                sides(ui, top, right, bottom, left);
                ui.end_row();
                ui.label("Width");
                ui.add(egui::Slider::new(width, 2..=200).suffix(" px"));
                ui.end_row();
            });
            Some(("effect.fadedEdge", json!({"top": top, "right": right, "bottom": bottom, "left": left, "width": width})))
        }
        Dialog::Bevel { width, strength } => {
            settings(ui, "bevel", |ui| {
                ui.label("Width");
                ui.add(egui::Slider::new(width, 1..=60).suffix(" px"));
                ui.end_row();
                ui.label("Strength");
                ui.add(egui::Slider::new(strength, 0..=100));
                ui.end_row();
            });
            Some(("effect.bevel", json!({"width": width, "strength": strength})))
        }
        Dialog::Perspective { horizontal, vertical } => {
            settings(ui, "perspective", |ui| {
                ui.label("Sideways");
                ui.add(egui::Slider::new(horizontal, -100..=100)).on_hover_text("Above nought the right side goes away; below, the left");
                ui.end_row();
                ui.label("Up and down");
                ui.add(egui::Slider::new(vertical, -100..=100)).on_hover_text("Above nought the bottom goes away; below, the top");
                ui.end_row();
            });
            Some(("effect.perspective", json!({"horizontal": horizontal, "vertical": vertical})))
        }
        Dialog::Curl { corner, size } => {
            settings(ui, "curl", |ui| {
                ui.label("Corner");
                egui::ComboBox::from_id_salt("curl-corner").selected_text(corner.label()).show_ui(ui, |ui| {
                    for c in Corner::ALL {
                        ui.selectable_value(corner, c, c.label());
                    }
                });
                ui.end_row();
                ui.label("Size");
                ui.add(egui::Slider::new(size, 5..=60).suffix(" %"));
                ui.end_row();
            });
            Some(("effect.pageCurl", json!({"corner": corner, "size": size})))
        }
        Dialog::Reflection { height, opacity, gap } => {
            settings(ui, "reflection", |ui| {
                ui.label("Height");
                ui.add(egui::Slider::new(height, 5..=100).suffix(" %"));
                ui.end_row();
                ui.label("Opacity");
                ui.add(egui::Slider::new(opacity, 0..=100).suffix(" %"));
                ui.end_row();
                ui.label("Gap");
                ui.add(egui::Slider::new(gap, 0..=60).suffix(" px"));
                ui.end_row();
            });
            Some(("effect.reflection", json!({"height": height, "opacity": opacity, "gap": gap})))
        }
        Dialog::Hue { hue, saturation, lightness, gamma } => {
            settings(ui, "hue", |ui| {
                ui.label("Hue");
                ui.add(egui::Slider::new(hue, -180.0..=180.0).suffix("°"));
                ui.end_row();
                ui.label("Saturation");
                ui.add(egui::Slider::new(saturation, -100.0..=100.0));
                ui.end_row();
                ui.label("Lightness");
                ui.add(egui::Slider::new(lightness, -100.0..=100.0));
                ui.end_row();
                ui.label("Gamma");
                ui.add(egui::Slider::new(gamma, 0.2..=5.0).logarithmic(true)).on_hover_text("Above 1 lightens the mid-tones, below 1 darkens them");
                ui.end_row();
            });
            Some(("effect.hueSaturation", json!({"hue": hue, "saturation": saturation, "lightness": lightness, "gamma": gamma})))
        }
        Dialog::Replace { from, with, tolerance, joined, at } => {
            let ctx = ui.ctx().clone();
            // A colour picked from the picture comes with the pixel it was picked on.
            let point = crate::eyedropper::take_point(&ctx, REPLACE_PICK);
            if let Some(c) = crate::eyedropper::take(&ctx, REPLACE_PICK) {
                *from = crate::to_color32(c.with_alpha(255));
                *at = point;
            }
            settings(ui, "replace-colour", |ui| {
                ui.label("Colour");
                ui.horizontal(|ui| {
                    if egui::color_picker::color_edit_button_srgba(ui, from, egui::color_picker::Alpha::Opaque).changed() {
                        *at = None;
                    }
                    let picking = crate::eyedropper::for_button(&ctx).is_some_and(|b| b == REPLACE_PICK);
                    let tip = "Pick the colour from the picture, on the part to change";
                    let resp = crate::icons::button_sized(ui, crate::icons::Icon::Eyedropper, tip, picking, true, 22.0);
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, REPLACE_PICK));
                    if resp.clicked() {
                        crate::eyedropper::start(&ctx, REPLACE_PICK);
                    }
                });
                ui.end_row();
                ui.label("With");
                egui::color_picker::color_edit_button_srgba(ui, with, egui::color_picker::Alpha::Opaque);
                ui.end_row();
                ui.label("Tolerance");
                ui.add(egui::Slider::new(tolerance, 0..=128));
                ui.end_row();
            });
            if at.is_none() {
                *joined = false;
            }
            ui.add_enabled(at.is_some(), egui::Checkbox::new(joined, REPLACE_JOINED))
                .on_hover_text("Only the part of that colour round the point picked, not the same colour elsewhere")
                .on_disabled_hover_text("Pick the colour from the picture first, on the part to change");
            ui.label(
                egui::RichText::new("Colours this close to it change too and keep their shade; smooth edges round it take the new colour.")
                    .small()
                    .color(Tokens::get(ui.ctx()).muted),
            );
            let mut params = json!({"color": hex(*from), "with": hex(*with), "tolerance": tolerance});
            if let (true, Some((x, y))) = (*joined, *at) {
                params["x"] = json!(x);
                params["y"] = json!(y);
                params["contiguous"] = json!(true);
            }
            Some(("effect.replaceColor", params))
        }
        Dialog::Sharpen { amount, radius } => {
            settings(ui, "sharpen", |ui| {
                ui.label("Amount");
                ui.add(egui::Slider::new(amount, 0..=300).suffix(" %"));
                ui.end_row();
                ui.label("Radius");
                ui.add(egui::Slider::new(radius, 0.5..=10.0).suffix(" px"));
                ui.end_row();
            });
            Some(("effect.sharpen", json!({"amount": amount, "radius": radius})))
        }
        Dialog::Blur { radius } => {
            settings(ui, "blur-all", |ui| {
                ui.label("Softness");
                ui.add(egui::Slider::new(radius, 0.5..=40.0).suffix(" px"));
                ui.end_row();
            });
            Some(("effect.blur", json!({"radius": radius})))
        }
        Dialog::Watermark { words, text, picture, choose, place, size, angle, opacity, under, color } => {
            settings(ui, "watermark", |ui| {
                ui.label("Of");
                ui.horizontal(|ui| {
                    ui.radio_value(words, true, "Words");
                    ui.radio_value(words, false, "A picture");
                });
                ui.end_row();
                if *words {
                    ui.label("Words");
                    ui.add(egui::TextEdit::singleline(text).desired_width(110.0).hint_text("DRAFT"));
                    ui.end_row();
                    ui.label("Colour");
                    egui::color_picker::color_edit_button_srgba(ui, color, egui::color_picker::Alpha::Opaque);
                    ui.end_row();
                } else {
                    ui.label("Picture");
                    ui.vertical(|ui| {
                        if let Some(name) = picture.as_deref() {
                            ui.label(egui::RichText::new(name).small());
                        }
                        if ui.button("Choose a Picture…").on_hover_text("Pick a picture file to use as the watermark").clicked() {
                            *choose = true;
                        }
                    });
                    ui.end_row();
                }
                ui.label("Where");
                egui::ComboBox::from_id_salt("watermark-place").selected_text(place.label()).show_ui(ui, |ui| {
                    for p in Place::ALL {
                        ui.selectable_value(place, p, p.label());
                    }
                });
                ui.end_row();
                ui.label("Size");
                ui.add(egui::Slider::new(size, 2..=100).suffix(" %")).on_hover_text("How much of the picture's width it takes");
                ui.end_row();
                ui.label("Angle");
                ui.add(egui::Slider::new(angle, -90..=90).suffix("°"));
                ui.end_row();
                ui.label("Opacity");
                ui.add(egui::Slider::new(opacity, 5..=100).suffix(" %"));
                ui.end_row();
                ui.label("Under");
                ui.checkbox(under, "its words and lines").on_hover_text("Shown on the picture's plain parts only, never over its words and lines");
                ui.end_row();
            });
            let params = if *words {
                json!({"text": text, "color": hex(*color), "place": place, "size": size, "angle": angle, "opacity": opacity, "under": under})
            } else {
                json!({"picture": 0, "place": place, "size": size, "angle": angle, "opacity": opacity, "under": under})
            };
            Some(("effect.watermark", params))
        }
        Dialog::Details { time, time_text, program, program_text, size, place, font_size, color, background } => {
            settings(ui, "details", |ui| {
                ui.checkbox(time, "Time");
                ui.add_enabled(*time, egui::TextEdit::singleline(time_text).desired_width(110.0).hint_text("when it was captured"));
                ui.end_row();
                ui.checkbox(program, "Of");
                ui.add_enabled(*program, egui::TextEdit::singleline(program_text).desired_width(110.0).hint_text("the program, if known"));
                ui.end_row();
                ui.checkbox(size, "Size");
                ui.label(egui::RichText::new("its width and height").color(Tokens::get(ui.ctx()).muted));
                ui.end_row();
                ui.label("Where");
                egui::ComboBox::from_id_salt("details-place").selected_text(place.label()).show_ui(ui, |ui| {
                    for p in DetailsPlace::ALL {
                        ui.selectable_value(place, p, p.label());
                    }
                });
                ui.end_row();
                ui.label("Text size");
                ui.add(egui::Slider::new(font_size, 8..=48).suffix(" px"));
                ui.end_row();
                ui.label("Colours");
                ui.horizontal(|ui| {
                    egui::color_picker::color_edit_button_srgba(ui, color, egui::color_picker::Alpha::Opaque).on_hover_text("The words");
                    egui::color_picker::color_edit_button_srgba(ui, background, egui::color_picker::Alpha::BlendOrAdditive)
                        .on_hover_text("Behind them");
                });
                ui.end_row();
            });
            // Left empty, the time and what it is of are as the editor knows them.
            let said = |on: bool, t: &str| {
                if !on {
                    json!(false)
                } else if t.trim().is_empty() {
                    json!(true)
                } else {
                    json!(t.trim())
                }
            };
            Some((
                "effect.captureDetails",
                json!({"time": said(*time, time_text), "program": said(*program, program_text), "size": size, "place": place, "fontSize": font_size, "color": hex(*color), "background": hex(*background)}),
            ))
        }
        Dialog::Shortcuts => {
            // Taller than a small window: the list scrolls, the footer stays in sight.
            let room = (ui.ctx().viewport_rect().height() - 200.0).max(160.0);
            egui::ScrollArea::vertical().max_height(room).show(ui, shortcuts);
            None
        }
        Dialog::VideoShortcuts => {
            let room = (ui.ctx().viewport_rect().height() - 200.0).max(160.0);
            egui::ScrollArea::vertical().max_height(room).show(ui, video_shortcuts);
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

/// The command a dialog's settings come to, as [`body`] gives it once drawn (Batch Changes keeps
/// them before they're drawn); none for a dialog with nothing to apply, or a watermark's picture
/// (it's read by the editor when it's chosen).
pub(crate) fn command(d: &Dialog) -> Option<(&'static str, Value)> {
    Some(match d {
        Dialog::Resize { width, height, .. } => ("image.resize", json!({"width": width, "height": height})),
        Dialog::Canvas { left, top, right, bottom, color } => {
            ("image.canvas", json!({"left": left, "top": top, "right": right, "bottom": bottom, "color": hex(*color)}))
        }
        Dialog::Border { width, color } => ("effect.border", json!({"width": width, "color": hex(*color)})),
        Dialog::Shadow { offset, softness, color } => ("effect.shadow", json!({"offset": offset, "softness": softness, "color": hex(*color)})),
        Dialog::Torn { top, right, bottom, left, depth } => {
            ("effect.tornEdge", json!({"top": top, "right": right, "bottom": bottom, "left": left, "depth": depth}))
        }
        Dialog::Round { radius } => ("effect.roundCorners", json!({"radius": radius})),
        Dialog::Brightness { brightness, contrast } => ("effect.brightness", json!({"brightness": brightness, "contrast": contrast})),
        Dialog::RotateAny { degrees, see_through, color } => {
            let mut params = json!({"degrees": degrees});
            if !*see_through {
                params["color"] = json!(hex(*color));
            }
            ("image.rotate", params)
        }
        Dialog::Wavy { top, right, bottom, left, depth, length } => {
            ("effect.wavyEdge", json!({"top": top, "right": right, "bottom": bottom, "left": left, "depth": depth, "length": length}))
        }
        Dialog::Faded { top, right, bottom, left, width } => {
            ("effect.fadedEdge", json!({"top": top, "right": right, "bottom": bottom, "left": left, "width": width}))
        }
        Dialog::Bevel { width, strength } => ("effect.bevel", json!({"width": width, "strength": strength})),
        Dialog::Perspective { horizontal, vertical } => ("effect.perspective", json!({"horizontal": horizontal, "vertical": vertical})),
        Dialog::Curl { corner, size } => ("effect.pageCurl", json!({"corner": corner, "size": size})),
        Dialog::Reflection { height, opacity, gap } => ("effect.reflection", json!({"height": height, "opacity": opacity, "gap": gap})),
        Dialog::Hue { hue, saturation, lightness, gamma } => {
            ("effect.hueSaturation", json!({"hue": hue, "saturation": saturation, "lightness": lightness, "gamma": gamma}))
        }
        Dialog::Replace { from, with, tolerance, joined, at } => {
            let mut params = json!({"color": hex(*from), "with": hex(*with), "tolerance": tolerance});
            if let (true, Some((x, y))) = (*joined, *at) {
                params["x"] = json!(x);
                params["y"] = json!(y);
                params["contiguous"] = json!(true);
            }
            ("effect.replaceColor", params)
        }
        Dialog::Sharpen { amount, radius } => ("effect.sharpen", json!({"amount": amount, "radius": radius})),
        Dialog::Blur { radius } => ("effect.blur", json!({"radius": radius})),
        Dialog::Watermark { words, text, place, size, angle, opacity, under, color, .. } => {
            if !*words {
                return None;
            }
            (
                "effect.watermark",
                json!({"text": text, "color": hex(*color), "place": place, "size": size, "angle": angle, "opacity": opacity, "under": under}),
            )
        }
        Dialog::Details { time, time_text, program, program_text, size, place, font_size, color, background } => {
            let said = |on: bool, t: &str| {
                if !on {
                    json!(false)
                } else if t.trim().is_empty() {
                    json!(true)
                } else {
                    json!(t.trim())
                }
            };
            (
                "effect.captureDetails",
                json!({"time": said(*time, time_text), "program": said(*program, program_text), "size": size, "place": place, "fontSize": font_size, "color": hex(*color), "background": hex(*background)}),
            )
        }
        Dialog::Shortcuts | Dialog::VideoShortcuts | Dialog::About => return None,
    })
}

/// Every key the video editor knows, and what the mouse does on its timeline.
fn video_shortcuts(ui: &mut Ui) {
    egui::Grid::new("video-keys").num_columns(2).striped(true).spacing([18.0, 4.0]).max_col_width(370.0).show(ui, |ui| {
        for (k, what) in [
            ("Space", "Play or pause"),
            ("Left / Right", "Back or on by one frame"),
            ("Home / End", "Go to the start or the end"),
            ("F11 or F", "The preview over the whole screen (Esc comes back)"),
            ("S", "Split at the playhead: the selected clip's track, or every track"),
            ("T", "Put words on the video at the playhead, then type them"),
            ("M", "Mark the moment at the playhead (a chapter of the video made)"),
            ("H", "Hold the frame at the playhead for 2 seconds (every track waits)"),
            ("Delete", "Delete what's selected, or cut the chosen stretch out of every track"),
            ("Ctrl+X", "Cut the chosen stretch"),
            ("Ctrl+A", "Choose the whole video"),
            ("Esc", "Let go of what's chosen"),
            ("Ctrl+Z / Ctrl+Y", "Undo / Redo"),
            ("Ctrl++ / Ctrl+-", "Zoom the timeline in or out"),
            ("Ctrl+0", "Fit the whole video on the timeline"),
            ("Ctrl+wheel", "Zoom the timeline around the pointer"),
            ("Wheel", "Scroll along the timeline"),
            ("Ctrl+E", "Export Video"),
            ("Ctrl+Shift+S", "Save the frame at the playhead as a picture (it opens in the Image Editor)"),
            ("Ctrl+O", "Open"),
            ("Click the ruler", "Move the playhead there"),
            ("Drag a clip", "Slide it along its track; drag one of its ends to trim it"),
            ("Drag a handle on the ruler", "Choose a stretch of the video (the handles ride either side of the playhead); Play then plays just that"),
            ("Drag across the tracks", "Choose a stretch of the video"),
            ("Alt while dragging", "Nothing catches on the playhead or on other clips' ends"),
            ("Right-click a track", "Split that track alone, take the chosen stretch out of it alone, choose, play, rename, move it"),
            ("Double-click a track's name", "Rename it (its words in the transcript take the name too)"),
            ("The arrows by a track's name", "Move it up or down (a layer higher up is in front)"),
            ("Right-click the ruler", "Split every track, cut the chosen stretch, choose, play, zoom"),
            ("Drag from the Library", "Put a title, an overlay, a transition or your own media on the timeline"),
            ("Double-click a Library card", "Add it at the playhead"),
        ] {
            ui.label(egui::RichText::new(k).monospace());
            ui.label(what);
            ui.end_row();
        }
    });
}

/// Every key the editor knows, and what it does.
fn shortcuts(ui: &mut Ui) {
    egui::Grid::new("keys").num_columns(2).striped(true).spacing([18.0, 4.0]).max_col_width(330.0).show(ui, |ui| {
        let mut row = |k: &str, what: &str| {
            ui.label(egui::RichText::new(k).monospace());
            ui.label(what);
            ui.end_row();
        };
        for (_, _, label, key) in crate::editor::Tool::ALL {
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
            ("Ctrl+G / Ctrl+Shift+G", "Group / Ungroup"),
            ("The round handle", "Turn the selected box or group (Shift: in steps of 15°)"),
            ("Ctrl+S / Ctrl+Shift+S", "Save / Save As"),
            ("Ctrl+E", "Export"),
            ("Ctrl+P", "Print"),
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
    if let Dialog::Watermark { choose, .. } = &mut d
        && std::mem::take(choose)
    {
        ed.choose_watermark();
    }
    if let Some((id, params)) = apply {
        ed.apply_effect(ctx, id, params);
        return;
    }
    if !close {
        ed.dialog = Some(d);
    }
}
